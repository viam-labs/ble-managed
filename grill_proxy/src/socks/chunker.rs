//! Defines a chunker.

use std::io::Cursor;

use anyhow::{anyhow, Result};
use async_channel::Receiver;
use log::debug;
use tokio::io::AsyncReadExt;

/// A chunker to read chunks of bytes from an `async_channel::Receiver`.
pub(crate) struct Chunker {
    reader: Receiver<Vec<u8>>,
    chunk: Cursor<Vec<u8>>,
}

impl Chunker {
    pub(crate) fn new(reader: Receiver<Vec<u8>>) -> Self {
        let chunk = Cursor::new(Vec::new()); // lazy
        Chunker { reader, chunk }
    }

    pub(crate) async fn read(&mut self, n: usize) -> Result<Vec<u8>> {
        let mut buffer = vec![0; n];

        // If chunk cursor is empty; grab new chunk.
        if self.chunk.position() as usize >= self.chunk.get_ref().len() {
            self.chunk = match self.reader.recv().await {
                Ok(chunk) => Cursor::new(chunk),
                Err(e) => {
                    return Err(anyhow!("could not get new chunk: {e}"));
                }
            };
            // TODO(benji): remove this debug
            debug!("Got a new chunk!");
        }

        if let Err(e) = self.chunk.read_exact(&mut buffer).await {
            return Err(anyhow!("could not read {n} bytes from current chunk: {e}"));
        }
        Ok(buffer)
    }
}
