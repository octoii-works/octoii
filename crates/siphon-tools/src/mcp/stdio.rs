//! A server run as a command, talked to over its standard input and output: what is written to it its input, taken by
//! Beak at once; what it writes to its output read as it tells it, ending as it does. Its errors Beak keeps, read back
//! where it could not be talked to.

use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use beak::{Command, CommandEvent, CommandEvents};
use bytes::Bytes;
use futures::Stream;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::io::StreamReader;

/// What the command writes to its output, read as it tells it, ending as it does.
pub fn output(events: CommandEvents) -> impl AsyncRead + Send + Unpin + 'static {
    let told = futures::stream::unfold(events, |mut events| async move {
        loop {
            match events.next().await? {
                CommandEvent::Output(bytes) => return Some((Ok::<Bytes, io::Error>(bytes), events)),
                CommandEvent::Title(_) => {}
                CommandEvent::Exit(_) => return None,
            }
        }
    });
    let told: Pin<Box<dyn Stream<Item = io::Result<Bytes>> + Send>> = Box::pin(told);
    StreamReader::new(told)
}

/// The command's input, written as it is given.
pub struct Input(pub Arc<Command>);

impl AsyncWrite for Input {
    fn poll_write(self: Pin<&mut Self>, _: &mut Context<'_>, bytes: &[u8]) -> Poll<io::Result<usize>> {
        Poll::Ready(self.0.write(bytes).map(|()| bytes.len()).map_err(io::Error::other))
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(self.0.close_input().map_err(io::Error::other))
    }
}
