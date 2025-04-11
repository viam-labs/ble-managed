//! Defines a chunker.

use anyhow::{anyhow, Result};
use async_channel::Receiver;
use tokio::io::AsyncWriteExt;

/// A chunker to read chunks of bytes from an `async_channel::Receiver`.
pub(crate) struct Chunker {
    reader: Receiver<Vec<u8>>,
    chunk: Vec<u8>,
}

impl Chunker {
    pub(crate) fn new(reader: Receiver<Vec<u8>>) -> Self {
        Chunker {
            reader,
            chunk: Vec::new(),
        }
    }

    pub(crate) async fn read(&mut self, n: usize) -> Result<Vec<u8>> {
        // While chunk does not have enough bytes for read; grab new chunks.
        while self.chunk.len() < n {
            let new_chunk =
                match tokio::time::timeout(std::time::Duration::from_secs(10), self.reader.recv())
                    .await
                {
                    Ok(Ok(chunk)) => chunk,
                    Ok(Err(e)) => {
                        return Err(anyhow!("could not get new chunk: {e}"));
                    }
                    Err(_) => {
                        // We should be able to read _some_ chunk ~ every second, as keepalives
                        // should be being sent from the flutter side at 1hz. Error in the case
                        // that we go 10s without receiving any keepalive.
                        return Err(anyhow!("timed out waiting for new chunk"));
                    }
                };

            match self.chunk.write(&new_chunk).await {
                Ok(n) if n > 0 => {}
                Ok(_) => {
                    return Err(anyhow!("overflowed writing to chunk"));
                }
                Err(e) => {
                    return Err(anyhow!("could not write new chunk: {e}"));
                }
            }
        }

        Ok(self.chunk.drain(0..n).collect())
    }
}
