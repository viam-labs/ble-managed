use bluer::l2cap;

use anyhow::{anyhow, Result};
use crossbeam_channel::{Receiver, Sender};
use dashmap::DashMap;
use log::{debug, error, info};
use std::sync::{
    atomic::{AtomicU16, Ordering::Relaxed},
    Arc,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, ReadHalf, WriteHalf},
    net::TcpStream,
    task::JoinHandle,
};

/// Value to set for incoming maximum-transmission-unit on created L2CAP streams.
const RECV_MTU: u16 = 65535;

/// A "socket" multiplexer that owns the underlying L2CAP stream.
struct L2CAPStreamSocketMultiplexer {
    // Next port to assign to an incoming socket.
    next_port: AtomicU16,
    // Map of ports to sockets.
    port_to_socket: Arc<DashMap<u16, L2CAPStreamedSocket>>,
    // L2CAP stream. ReadHalf passed directly to `pipe_reads_into_chunks`.
    l2cap_stream_write: Arc<WriteHalf<l2cap::Stream>>,
    // Chunks from L2CAP to be read by sockets.
    chunks_send: Arc<Sender<Vec<u8>>>,
    chunks_receive: Arc<Receiver<Vec<u8>>>,
    // Group of tasks.
    tasks: Vec<JoinHandle<()>>,
    // Whether mux has been stopped.
    stopped: bool,
}

impl L2CAPStreamSocketMultiplexer {
    /// Creates new mux.
    pub(crate) fn create_and_start(stream: l2cap::Stream) -> Self {
        let next_port = AtomicU16::new(0);
        let port_to_socket = Arc::new(DashMap::default());

        let (l2cap_stream_read, l2cap_stream_write) = tokio::io::split(stream);
        let (chunks_send, chunks_receive) = crossbeam_channel::unbounded::<Vec<u8>>();

        let tasks = Vec::with_capacity(4);

        let mut mux = L2CAPStreamSocketMultiplexer {
            next_port,
            port_to_socket,
            l2cap_stream_write: Arc::new(l2cap_stream_write),
            chunks_send: Arc::new(chunks_send),
            chunks_receive: Arc::new(chunks_receive),
            tasks,
            stopped: false,
        };

        mux.pipe_reads_into_chunks(l2cap_stream_read);
        mux.read_chunks_to_sockets();
        mux.pipe_writes_into_chan();
        mux.send_keep_alive_frames_forever();

        mux
    }

    /// Adds a new "socket" to be multiplexed.
    pub(crate) async fn add_socket(&mut self, stream: TcpStream) -> Result<()> {
        if self.stopped {
            return Err(anyhow!("cannot add new socket; already closed"));
        }

        if self.next_port.load(Relaxed) > 65535 {
            self.next_port.store(0, Relaxed);
        }
        let port = self.next_port.fetch_add(1, Relaxed);

        if self.port_to_socket.contains_key(&port) {
            return Err(anyhow!("Too many open connections"));
        }

        let (read, write) = tokio::io::split(stream);
        let socket = L2CAPStreamedSocket {
            port,
            tcp_stream_read: Some(read),
            tcp_stream_write: write,
        };
        self.port_to_socket.insert(port, socket);

        // Client MUST disclose its port first with a CONTROL packet.
        let control_packet = Packet::control_socket_open(port).await?;
        let serialized_cp = control_packet.serialize().await?;
        let mut l2cap_stream_write = self.l2cap_stream_write.lock().await;
        if let Err(e) = l2cap_stream_write.write_all(&serialized_cp).await {
            return Err(anyhow!("error writing to TCP stream: {e}"));
        }

        Ok(())
    }

    /// Stops the mux.
    pub(crate) async fn stop(&mut self) -> Result<()> {
        self.stopped = true;

        while let Some(task) = self.tasks.pop() {
            // TODO: more cleanly shut down tasks with a cancelation signal instead of abort.
            task.abort();
        }
        Ok(())
    }

    /// Reads from network into chunks.
    fn pipe_reads_into_chunks(&mut self, mut l2cap_stream_read: ReadHalf<l2cap::Stream>) {
        let chunks_send = self.chunks_send.clone();
        let handler = tokio::spawn(async move {
            loop {
                // TODO: use 256 as chunk cap instead?
                let mut chunk_buf = vec![0u8; RECV_MTU as usize];
                match l2cap_stream_read.read(&mut chunk_buf).await {
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
                if let Err(e) = chunks_send.send(chunk_buf) {
                    error!("Error sending to chunks channel: {e}");
                    break;
                }
            }
        });
        self.tasks.push(handler);
    }

    /// Reads chunks into sockets.
    fn read_chunks_to_sockets(&mut self) {
        let chunks_receive = self.chunks_receive.clone();
        let port_to_socket = self.port_to_socket.clone();
        let handler = tokio::spawn(async move {
            loop {
                let pkt = match Packet::deserialize(chunks_receive.clone()).await {
                    Ok(pkt) => pkt,
                    Err(e) => {
                        error!("Error deserializing packet: {e}");
                        break;
                    }
                };
                match pkt {
                    Packet::Data { port, data } => {
                        if data.len() == 0 {
                            continue;
                        }

                        let mut socket = match port_to_socket.get_mut(&port) {
                            Some(socket) => socket,
                            None => {
                                debug!("Unknown port $port; dropping packet");
                                continue;
                            }
                        };

                        if let Err(e) = socket.tcp_stream_write.write(&data).await {
                            error!("Error writing to socket {}: {e}", socket.port);
                            break;
                        }
                    }
                    Packet::Control {
                        msg_type,
                        for_port,
                        status,
                        ..
                    } => {
                        if msg_type == 0 {
                            // Keep alive.
                            continue;
                        }
                        if msg_type != 0 {
                            error!(
                                "Do not know how to handle MSG_TYPE {msg_type}; dropping packet"
                            );
                            continue;
                        }

                        match status {
                            0 => {
                                // Closed.
                                let mut socket = match port_to_socket.get_mut(&for_port) {
                                    Some(socket) => socket,
                                    None => {
                                        debug!("Unknown port $for_port; dropping packet");
                                        continue;
                                    }
                                };
                                socket.close_read();
                                port_to_socket.remove(&for_port);
                            }
                            1 => {
                                // Open.
                                error!("Cannot accept request to open socket on this side");
                                continue;
                            }
                            _ => {
                                error!(
                                    "Do not know how to handle FOR_PORT {for_port} STATUS {status}"
                                );
                                continue;
                            }
                        }
                    }
                };
            }
        });
        self.tasks.push(handler);
    }

    /// Pipes writes from all sockets into the network.
    fn pipe_writes_into_chan(&mut self) {
        let handler = tokio::spawn(async move {});
        self.tasks.push(handler);
    }

    /// Sends keep alives.
    fn send_keep_alive_frames_forever(&mut self) {}
}

enum Packet {
    Data {
        port: u16,
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
    async fn deserialize(chunks_receive: Arc<Receiver<Vec<u8>>>) -> Result<Self> {
        let message_buf = Vec::with_capacity(2);
        let n = chunks_receive.recv_many(&mut message_buf, 2).await;
        if n != 2 {
            return Err(anyhow!("expected 2 bytes for PORT but got {n}"));
        }
        // Decode PORT from first two bytes.

        Ok(())
    }

    /*
    +------+-----+------+
    | PORT | LEN | DATA |
    +------+-----+------+
    |   2  |  4  | LEN  |
    +------+-----+------+
    */
    async fn serialize(&self) -> Result<Vec<u8>> {
        let data = match self {
            Packet::Data { port, data } => {
                let data_length = data.len();
                if data_length > 4294967295 {
                    return Err(anyhow!("data too large to send {}", data_length));
                }

                let mut length_and_data = vec![0u8; 2 + 4 + data_length];
                // TODO: use little endian encoding?
                length_and_data.write_u16(port.to_owned()).await?;
                length_and_data.write_u32(data_length as u32).await?;
                length_and_data.write_all(data).await?;
                length_and_data
            }
            Packet::Control { raw_data, .. } => raw_data.to_owned(),
        };

        Ok(data)
    }

    /*
     Connection Status

     +------+----------+----------+--------+
     | PORT | MSG_TYPE | FOR_PORT | STATUS |
     +------+----------+----------+--------+
     | 2=0  |  1=1     | 2        |    1   |
     +------+----------+----------+--------+

    3 bytes for:
    Port, Status

    Status 0 = Closed
    Status 1 = Open
    */
    async fn control_socket_open(for_port: u16) -> Result<Self> {
        let mut raw_data = vec![0u8; 6];
        // TODO: specify endian ordering?
        raw_data.write_u16(0).await?;
        raw_data.write_u8(1).await?;
        raw_data.write_u16(for_port).await?;
        raw_data.write_u8(1).await?;

        Ok(Self::Control {
            msg_type: 1,
            for_port,
            status: 1,
            raw_data,
        })
    }
}

/// A "socket" to be multiplexed over an L2CAP stream by an `L2CAPStreamMultiplexer`.
struct L2CAPStreamedSocket {
    port: u16,
    tcp_stream_read: Option<ReadHalf<TcpStream>>,
    tcp_stream_write: WriteHalf<TcpStream>,
}

impl L2CAPStreamedSocket {
    fn close_read(&mut self) {
        // Setting to `None` will drop the underlying value. Cannot manually drop a field within a
        // struct.
        self.tcp_stream_read = None;
    }
}
