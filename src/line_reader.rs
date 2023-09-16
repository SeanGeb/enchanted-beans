use std::io;

use bytes::{Bytes, BytesMut};
use itertools::Itertools;
use tokio::io::{AsyncRead, AsyncReadExt};

/// Provides a facility to read CRLF-terminated lines from a stream.
///
/// In future this could be an `AsyncIterator<Item = Bytes>`.
pub struct LineReader<T: AsyncRead + Unpin> {
    /// Stores data that's been read in but lacks a CRLF.
    buf: BytesMut,
    /// Index in buf from which a valid CRLF pair may appear (and before which
    /// a CRLF sequence hasn't been seen).
    maybe_crlf_from: usize,
    /// Data source
    reader: T,
    /// On a reading error, this field is set and its value returned once the
    /// buffer is drained of pending lines.
    pending_error: Option<io::Error>,
}

impl<T: AsyncRead + Unpin> LineReader<T> {
    /// Reads a line from the internal buffer and/or reader. On an end-of-stream
    /// condition, returns a None result, discarding any partly-read line in the
    /// internal buffer.
    ///
    /// This function is cancel-safe: its only async operation is a `read_buf`
    /// against the internal `reader`, and so it has the same guarantees:
    /// either a complete read occurs and is processed, or this is cancelled.
    ///
    /// On a read error, the error value is returned after processing all
    /// pending lines in the internal buffer, but calling `read_line` again will
    /// attempt a new read safely.
    pub async fn read_line(&mut self) -> io::Result<Option<Bytes>> {
        loop {
            // We slice and dice buf here to avoid re-reading all but the last
            // byte of the part of the command we've already seen, keeping
            // O(bytes_read) behaviour.
            // Note also we need to scan from one position earlier than the
            // start of the newest bytes in case we received a \r then \n on the
            // next read.
            // The outer loop ensures pipelined line that arrive in the same
            // read_buf call are handled correctly: we only call read_buf once
            // all pending lines in the internal buffer have been removed.
            if let Some(eoc) = self
                .buf
                .iter()
                .skip(self.maybe_crlf_from)
                .tuple_windows::<(_, _)>()
                .position(|x| x == (&b'\r', &b'\n'))
            {
                // This should be a complete command. Freeze the result to make it
                // read-only.
                let cmd =
                    self.buf.split_to(self.maybe_crlf_from + eoc + 2).freeze();

                // Drop trailing b"\r\n".
                let cmd = cmd.slice(0..cmd.len() - 2);

                // Zero out the maybe_crlf_from position so we restart scanning for
                // commands from the start of the unread buffer section.
                self.maybe_crlf_from = 0;

                return Ok(Some(cmd));
            } else {
                // Try reading from the reader and accumulating in the buffer;
                // if we receive any bytes, re-scan for a CRLF, otherwise
                // assume the connection is dead/closed.
                let n_bytes_read =
                    match self.reader.read_buf(&mut self.buf).await {
                        Ok(n) => n,
                        Err(e) => {
                            self.pending_error = Some(e);
                            0
                        },
                    };

                // Slightly convoluted, but all this does is set maybe_crlf_from
                // to the byte before the first byte returned in the read_buf
                // call (and 0 if buf is empty).
                self.maybe_crlf_from =
                    self.buf.len().checked_sub(n_bytes_read + 1).unwrap_or(0);

                // If we didn't read any bytes this time around, assume we've
                // reached an end-of-stream condition. Return any pending error:
                // we wouldn't be able to parse out another line, given we just
                // read 0 bytes.
                // TODO: write some tests for this behaviour.
                if n_bytes_read == 0 {
                    return match self.pending_error.take() {
                        Some(e) => Err(e),
                        None => Ok(None),
                    };
                }
            }
        }
    }
}

impl<T: AsyncRead + Unpin> From<T> for LineReader<T> {
    fn from(value: T) -> Self {
        Self {
            buf: BytesMut::new(),
            maybe_crlf_from: 0,
            reader: value,
            pending_error: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use tokio::io::{self, AsyncWriteExt};
    use tokio::task::yield_now;

    #[tokio::test]
    async fn test() {
        // When properly read, each nth line should read b"test:{n}".
        let tests: &[&[u8]] = &[
            // Simple reassembly
            b"test:",
            b"1\r\n",
            // Split LF
            b"test:",
            b"2\r",
            b"\n",
            // Split CRLF
            b"test:",
            b"3",
            b"\r",
            b"\n",
            // Pipelined commands
            // Simple
            b"test:4\r\ntest:5\r\n",
            // Split LF
            b"test:6\r",
            b"\ntest:7\r\n",
            // Split CRLF
            b"test:8",
            b"\r\ntest:9\r\n",
        ];

        // Set the buffer large enough that our tests will never overflow it.
        // We can ensure correct fragmentation of reads by explicitly yielding
        // between each.
        let (mut client, server) = io::duplex(4096);

        tokio::spawn(async move {
            for buf in tests {
                client.write_all(buf).await.unwrap();
                yield_now().await;
            }
        });

        let mut lr: LineReader<_> = server.into();

        for n in 1..=9 {
            assert_eq!(
                lr.read_line().await.unwrap().unwrap(),
                format!("test:{n}")
            );
        }

        assert!(lr.read_line().await.unwrap().is_none());
    }
}
