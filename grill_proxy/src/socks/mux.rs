use bluer::l2cap;

use anyhow::{anyhow, Result};
use log::{debug, error, info};
use std::collections::HashMap;
use std::sync::atomic::AtomicU16;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, ReadHalf, WriteHalf},
    sync::mpsc::{self, Receiver, Sender},
    task::JoinHandle,
};

/// Value to set for incoming maximum-transmission-unit on created L2CAP streams.
const RECV_MTU: u16 = 65535;

/// A "socket" multiplexer that owns the underlying L2CAP stream.
struct L2CAPStreamSocketMultiplexer {
    // TODO(replace with dashmap)
    port_to_socket: HashMap<u16, L2CAPStreamedSocket>,
    next_port: AtomicU16,
    tasks: Vec<JoinHandle<()>>,
}

impl L2CAPStreamSocketMultiplexer {
    /// Creates new mux.
    pub(crate) fn new(stream: l2cap::Stream) -> Self {
        let port_to_socket = HashMap::default();
        let next_port = AtomicU16::new(0);
        let (stream_read, stream_write) = tokio::io::split(stream);
        let tasks = Vec::with_capacity(4);
        // TODO(Bound chunks channel differently)
        let (chunks_send, chunks_receive) = mpsc::channel::<Vec<u8>>(RECV_MTU as usize);

        let mut mux = L2CAPStreamSocketMultiplexer {
            port_to_socket,
            next_port,
            tasks,
        };

        mux.pipe_reads_into_chunks(stream_read, chunks_send);
        mux.read_chunks_to_sockets();

        mux
    }

    /// Reads from network into chunks.
    fn pipe_reads_into_chunks(
        &mut self,
        mut stream_read: ReadHalf<l2cap::Stream>,
        chunks_send: Sender<Vec<u8>>,
    ) {
        let handler = tokio::spawn(async move {
            loop {
                let mut chunk_buf = vec![0u8; RECV_MTU as usize];
                match stream_read.read(&mut chunk_buf).await {
                    Ok(n) if n > 0 => n,
                    Ok(_) => {
                        info!("L2CAP stream closed");
                        break;
                    }
                    Err(e) => {
                        error!("Error reading from L2CAP stream: {e}");
                        break;
                    }
                };
                if let Err(e) = chunks_send.send(chunk_buf).await {
                    error!("Error sending to chunks channel: {e}");
                    break;
                }
            }
        });
        self.tasks.push(handler);
    }

    /// Reads chunks into sockets.
    fn read_chunks_to_sockets(&mut self) {
        let handler = tokio::spawn(async move {
            loop {
                // Read from chunk_reader and deserialize as a packet.
            }
        });
        self.tasks.push(handler);
    }

    /// Pipes writes from all sockets into the network.
    fn pipe_writes_into_chan(&mut self) -> Result<()> {
        Ok(())
    }

    /// Sends keep alives.
    fn send_keep_alive_frames_forever(&mut self) -> Result<()> {
        Ok(())
    }
}

enum Packet {
    Data {
        port: u8,
        data: Vec<u8>,
    },
    Control {
        msg_type: u8,
        for_port: u16,
        status: u8,
        raw_data: Vec<u8>,
    },
}

impl Packet {
    async fn deserialize(chunks_receive: Receiver<Vec<u8>>) -> Result<Self> {
        let message_buf = Vec::with_capacity(2);
        let n = chunks_receive.recv_many(&mut message_buf, 2).await;
        if n != 2 {
            return Err(anyhow!("expected 2 bytes for PORT but got {n}"));
        }
        // Decode PORT from first two bytes.

        Ok(())
    }

    fn serialize(&self) -> Result<Vec<u8>> {
        Ok(())
    }
}

/// A "socket" to be multiplexed over an L2CAP stream by an `L2CAPStreamMultiplexer`.
struct L2CAPStreamedSocket {
    port: u16,
}
