mod args;
mod util;

use std::process::ExitCode;

use anyhow::{Context, Result};
use bytes::BytesMut;
use clap::Parser;
use enchanted_beans::types::protocol::BeanstalkCommand;
use enchanted_beans::types::serialisable::BeanstalkSerialisable;
use itertools::Itertools;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::{select, signal};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, instrument, trace, warn, Level};

use crate::args::Args;
use crate::util::bytes_to_human_str;

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
    let mut buf = BytesMut::with_capacity(224);

    loop {
        let bytes_read = select! {
            n = conn.read_buf(&mut buf) => n.context("reading")?,
            _ = cancel.cancelled() => return Ok(()),
        };

        // We slice and dice buf here to avoid re-reading all but the last byte
        // of the part of the command we've already seen, keeping O(bytes_read)
        // behaviour.

        // We need to scan from one position earlier than the start of the
        // newest bytes in case we received a \r then \n on the next read.
        // We also need to be able to correctly handle command pipelining, where
        // multiple commands are sent in the same packet (e.g. "use tube"
        // followed by a "stats-tube" as b"use tube\r\nstats-tube\r\n").

        // Testing: all the following should work.
        // * b"hello" + b"world\r\n"
        // * b"hello" + b"world\r" + b"\n"
        // * b"hello" + b"world" + b"\r" + b"\n"
        // * b"hello\r\nworld\r\n"
        let mut maybe_crlf_from =
            buf.len().checked_sub(bytes_read + 1).unwrap_or(0);

        while let Some(eoc) = buf
            .iter()
            .skip(maybe_crlf_from)
            .tuple_windows::<(_, _)>()
            .position(|x| x == (&b'\r', &b'\n'))
        {
            // This should be a complete command.
            let cmd = buf.split_to(maybe_crlf_from + eoc + 2);
            // Drop trailing b"\r\n".
            let cmd = &cmd[0..cmd.len() - 2];
            trace!(cmd = bytes_to_human_str(cmd), "processing command");

            let resp = match TryInto::<BeanstalkCommand>::try_into(cmd) {
                Ok(c) => b"CMD_OK\r\n".to_vec(),
                Err(e) => e.serialise_beanstalk(),
            };

            // Slightly convoluted, but ensures we write out the buffer properly
            // with cancel safety.
            let mut resp_buf = &resp[..];
            select! {
                n = conn.write_all_buf(&mut resp_buf) => n?,
                _ = cancel.cancelled() => return Ok(()),
            };

            // Zero out the maybe_crlf_from position so we restart scanning for
            // commands from the start of the unread buffer section.
            maybe_crlf_from = 0;
        }

        // Handle a client disconnect here, so a client that sends a command
        // then immediately closes the sending side of its connection has its
        // last command acknowledged.
        if bytes_read == 0 {
            return Ok(());
        }
    }
}
