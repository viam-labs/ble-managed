//! Defines a chunker.

use std::io::Cursor;

use anyhow::{anyhow, Result};
use async_channel::Receiver;
use log::debug;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// A chunker to read chunks of bytes from an `async_channel::Receiver`.
pub(crate) struct Chunker {
    reader: Receiver<Vec<u8>>,
    cursor: Cursor<Vec<u8>>,
}

impl Chunker {
    pub(crate) fn new(reader: Receiver<Vec<u8>>) -> Self {
        let cursor = Cursor::new(Vec::new()); // lazy
        Chunker { reader, cursor }
    }

    pub(crate) async fn read(&mut self, n: usize) -> Result<Vec<u8>> {
        let mut buffer = vec![0; n];

        // If chunk cursor is empty or does not have enough bytes for read; grab new chunk.
        if self.cursor.position() as usize >= self.cursor.get_ref().len() - n {
            let chunk = match self.reader.recv().await {
                Ok(chunk) => chunk,
                Err(e) => {
                    return Err(anyhow!("could not get new chunk: {e}"));
                }
            };
            // TODO(benji): remove this debug
            debug!("Got a new chunk!");
            if let Err(e) = self.cursor.write(&chunk).await {
                return Err(anyhow!("could not write new chunk: {e}"));
            }
        }

        if let Err(e) = self.cursor.read_exact(&mut buffer).await {
            return Err(anyhow!("could not read {n} bytes from current chunk: {e}"));
        }
        Ok(buffer)
    }
}
