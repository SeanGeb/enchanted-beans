mod args;

use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::Parser;
use enchanted_beans::line_reader::LineReader;
use enchanted_beans::parser::ParsingError;
use enchanted_beans::types::protocol::BeanstalkCommand;
use enchanted_beans::types::serialisable::BeanstalkSerialisable;
use enchanted_beans::util::bytes_to_human_str;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::{select, signal};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, instrument, trace, warn, Level};

use crate::args::Args;

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let args = Args::parse();

    // Logging
    if args.debug {
        tracing_subscriber::fmt()
            .with_max_level(Level::TRACE)
            .init();
    } else {
        tracing_subscriber::fmt().json().init();
    }

    if let Some(_wal_dir) = args.wal_dir {
        error!("unsupported configuration: WAL not yet implemented");
        return ExitCode::from(2);
    }

    // Cancellation and termination channel.
    // TODO: this termination channel is a mpsc - so could be used to provide
    // durability.
    let cancel = CancellationToken::new();
    {
        let cancel = cancel.clone();
        tokio::spawn(async move {
            if let Err(error) = signal::ctrl_c().await {
                warn!(%error, "something strange with ctrl-c handling!");
            };
            cancel.cancel();
        });
    }

    let (shutdown_hold, mut shutdown_wait) = mpsc::channel::<()>(1);

    let exit_code = if let Err(error) = begin(args, cancel, shutdown_hold).await
    {
        error!(%error, "encountered runtime error");
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    };

    shutdown_wait.recv().await;

    exit_code
}

async fn begin(
    args: Args,
    cancel: CancellationToken,
    shutdown_hold: mpsc::Sender<()>,
) -> Result<()> {
    let listener = TcpListener::bind((args.listen, args.port)).await?;
    info!(addr = %listener.local_addr()?, "listening");

    // Accept incoming connections until an exit signal is sent, and handle each
    // connection as its own task.
    loop {
        let conn = match select! {
            accept = listener.accept() => accept,
            _ = cancel.cancelled() => break,
        } {
            Ok((conn, _)) => conn,
            Err(error) => {
                warn!(%error, "failed to accept connection");
                continue;
            },
        };

        tokio::spawn(begin_handle(cancel.clone(), shutdown_hold.clone(), conn));
    }

    Ok(())
}

#[instrument(name = "handle", err, fields(peer = %conn.peer_addr()?), skip_all)]
async fn begin_handle(
    cancel: CancellationToken,
    _shutdown_hold: mpsc::Sender<()>,
    mut conn: TcpStream,
) -> Result<()> {
    debug!("accepted connection");

    conn.set_nodelay(true).context("setting NODELAY")?;

    let ret = handle_conn(cancel, &mut conn).await;

    conn.shutdown().await.context("during shutdown")?;

    debug!("closed connection");

    ret
}

async fn handle_conn(
    cancel: CancellationToken,
    conn: &mut TcpStream,
) -> Result<()> {
    // Split conn into read and write halves, where the read half uses our
    // LineReader.
    let (r, mut w) = conn.split();
    let mut r: LineReader<_> = r.into();

    // Keep taking lines and parsing and processing them.
    loop {
        let line = select!(
           x = r.read_line() => match x? {
                Some(x) => x,
                None => return Ok(()),
           },
           _ = cancel.cancelled() => return Ok(()),
        );

        trace!(line = bytes_to_human_str(&line), "processing command");

        let cmd: Result<BeanstalkCommand, ParsingError> =
            (&line as &[u8]).try_into();

        // Slightly convoluted, but ensures we write out the buffer properly
        // with cancel safety.
        match cmd {
            Ok(_cmd) => select! {
                x = w.write_all(b"CMD_OK\r\n") => x,
                _ = cancel.cancelled() => return Ok(()),
            },
            Err(error) => {
                let resp = error.serialise_beanstalk();
                select! {
                    x = w.write_all(&resp) => x,
                    _ = cancel.cancelled() => return Ok(()),
                }
            },
        }?;

        // Flush any buffered packets once we've written out the one or more
        // responses. This provides a pipelined response to a pipelined request.
        // NB: flush() appears not to be implemented for TCPStreams, but this
        // should provide forward-compatibility for other transports.
        select! {
            x = w.flush() => x?,
            _ = cancel.cancelled() => return Ok(()),
        };
    }
}
