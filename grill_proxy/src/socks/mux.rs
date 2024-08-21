//! Implements one side of the multiplexing protocol defined in the following specification.
//! https://github.com/viamrobotics/flutter-ble/blob/bbe7e2a511c452f932c52e3784d7dca3751a03bd/doc/sockets.md

use anyhow::{anyhow, Result};
use async_channel::{self, Receiver, Sender};
use bluer::l2cap;
use dashmap::DashMap;
use log::{debug, error, info, trace, warn};
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

/// A multiplexer that allows sharing one L2CAP stream between multiple TCP streams.
pub(crate) struct L2CAPStreamMux {
    // Next "port" to assign to an incoming TCP stream.
    next_port: AtomicU16,
    // Map of "ports" to TCP streams.
    port_to_tcp_stream: Arc<DashMap<u16, MuxedTCPStream>>,

    // Raw "chunks" of data from L2CAP to be read by TCP streams.
    l2cap_to_tcp_send: Arc<Sender<Vec<u8>>>,
    l2cap_to_tcp_receive: Arc<Receiver<Vec<u8>>>,

    // `Packet`s from TCP streams to send to L2CAP stream.
    tcp_to_l2cap_send: Arc<Sender<Packet>>,
    tcp_to_l2cap_receive: Arc<Receiver<Packet>>,

    // Group of tasks.
    tasks: Vec<JoinHandle<()>>,
}

impl L2CAPStreamMux {
    /// Creates new mux from an L2CAP stream.
    pub(crate) fn create_and_start(stream: l2cap::Stream) -> Self {
        info!("Starting L2CAP stream multiplexer...");
        let next_port = AtomicU16::new(0);
        let port_to_tcp_stream = Arc::new(DashMap::default());

        let (l2cap_to_tcp_send, l2cap_to_tcp_receive) = async_channel::unbounded::<Vec<u8>>();
        let (tcp_to_l2cap_send, tcp_to_l2cap_receive) = async_channel::unbounded::<Packet>();

        let tasks = Vec::new();

        let mut mux = L2CAPStreamMux {
            next_port,
            port_to_tcp_stream,
            l2cap_to_tcp_send: Arc::new(l2cap_to_tcp_send),
            l2cap_to_tcp_receive: Arc::new(l2cap_to_tcp_receive),
            tcp_to_l2cap_send: Arc::new(tcp_to_l2cap_send),
            tcp_to_l2cap_receive: Arc::new(tcp_to_l2cap_receive),
            tasks,
        };

        let (l2cap_stream_read, l2cap_stream_write) = tokio::io::split(stream);

        mux.pipe_in_l2cap(l2cap_stream_read);
        mux.pipe_out_tcp();
        mux.pipe_in_tcp(l2cap_stream_write);
        mux.send_keepalive_frames_forever();

        info!("Started L2CAP stream multiplexer");
        mux
    }

    /// Incorporates a new TCP stream into the multiplexer.
    pub(crate) async fn add_tcp_stream(&mut self, stream: TcpStream) -> Result<()> {
        debug!("Adding new socket to multiplexer...");

        // Get new "port" value from atomic (start at 0 if overflow).
        if self.next_port.load(Relaxed) > 65534 {
            self.next_port.store(0, Relaxed);
        }
        let port = self.next_port.fetch_add(1, Relaxed);
        if self.port_to_tcp_stream.contains_key(&port) {
            return Err(anyhow!("too many open connections"));
        }

        let (mut tcp_stream_read, tcp_stream_write) = tokio::io::split(stream);
        let muxed_stream = MuxedTCPStream {
            writer: tcp_stream_write,
        };
        self.port_to_tcp_stream.insert(port, muxed_stream);

        // Send initial control packet to open.
        let control_packet = Packet::control_socket_open(port).await?;
        self.tcp_to_l2cap_send.send(control_packet).await?;

        let tcp_to_l2cap_send = self.tcp_to_l2cap_send.clone();
        // Spawn coroutine (and track it) to continue reading from TCP stream and writing to
        // 'tcp_to_l2cap' channel.
        let handler = tokio::spawn(async move {
            loop {
                debug!("Reading request from TCP stream for 'port' {port}...");
                // TODO: use a non-arbitrary cap here.
                let mut message_buf = vec![0u8; 1024];
                let n = match tcp_stream_read.read(&mut message_buf).await {
                    Ok(n) if n > 0 => n,
                    Ok(_) => {
                        debug!("TCP stream closed for 'port' {port}");
                        // Send a close control packet.
                        let control_packet = match Packet::control_socket_closed(port).await {
                            Ok(control_packet) => control_packet,
                            Err(e) => {
                                error!(
                                    "Could not create 'close' control packet for 'port' {port}: {e}"
                                );
                                break;
                            }
                        };
                        if let Err(e) = tcp_to_l2cap_send.send(control_packet).await {
                            error!("Could not send 'close' control packet for 'port' {port}: {e}");
                        }
                        break;
                    }
                    Err(e) => {
                        error!("Error reading from TCP stream; closing for 'port' {port}: {e}");
                        // Send a close control packet.
                        let control_packet = match Packet::control_socket_closed(port).await {
                            Ok(control_packet) => control_packet,
                            Err(e) => {
                                error!(
                                    "Could not create 'close' control packet for 'port' {port}: {e}"
                                );
                                break;
                            }
                        };
                        if let Err(e) = tcp_to_l2cap_send.send(control_packet).await {
                            error!("Could not send 'close' control packet for 'port' {port}: {e}");
                        }
                        break;
                    }
                };

                // Truncate message.
                message_buf.truncate(n);
                let length = message_buf.len();
                debug!("Writing request of length {length} from TCP stream with 'port' {port} as data packet...");
                trace!("Request message was {message_buf:#?}");

                let data_packet = Packet::Data {
                    port,
                    data: message_buf,
                };
                if let Err(e) = tcp_to_l2cap_send.send(data_packet).await {
                    error!("Error sending data packet to 'tcp_to_l2cap_send' channel; dropping data packet: {e}");
                    continue;
                }
            }
        });
        self.tasks.push(handler);

        debug!("Added new TCP stream with 'port' {port} to multiplexer");
        Ok(())
    }

    /// Reads from L2CAP stream into `l2cap_to_tcp`.
    fn pipe_in_l2cap(&mut self, mut l2cap_stream_read: ReadHalf<l2cap::Stream>) {
        let l2cap_to_tcp_send = self.l2cap_to_tcp_send.clone();
        let handler = tokio::spawn(async move {
            loop {
                let mut chunk_buf = vec![0u8; RECV_MTU as usize];
                match l2cap_stream_read.read(&mut chunk_buf).await {
                    Ok(n) if n > 0 => n,
                    // TODO: close all TCP streams in the event of L2CAP failure?
                    Ok(_) => {
                        info!("L2CAP stream closed");
                        break;
                    }
                    Err(e) => {
                        error!("Error reading from L2CAP stream: {e}");
                        break;
                    }
                };
                if let Err(e) = l2cap_to_tcp_send.send(chunk_buf).await {
                    error!("Error sending to 'l2cap_to_tcp' channel; dropping chunk: {e}");
                    continue;
                }
            }
        });
        self.tasks.push(handler);
    }

    // Read `n` bytes from `l2cap_to_tcp`; grabbing next chunk if necessary.
    // TODO(move to a new type).
    //async fn read_bytes(n: usize, reader: Arc<Receiver<) -> Result<Vec<u8>> {
    //let mut buffer = vec![0; n];

    //// If chunk cursor is empty; grab new chunk.
    //let pos = self.l2cap_to_tcp_curr_chunk.position() as usize;
    //let len = self.l2cap_to_tcp_curr_chunk.get_ref().len();
    //if pos >= len {
    //self.l2cap_to_tcp_curr_chunk = match self.l2cap_to_tcp_receive.recv().await {
    //Ok(chunk) => Cursor::new(chunk),
    //Err(e) => {
    //info!("Error receiving from 'l2cap_to_tcp' channel; likely closed: {e}");
    //return Ok(buffer);
    //}
    //}
    //}

    //if let Err(e) = self.l2cap_to_tcp_curr_chunk.read_exact(&mut buffer).await {
    //error!("Could not read {n} bytes from 'l2cap_to_tcp' channel: {e}");
    //}
    //Ok(buffer)
    //}

    /// Reads from `l2cap_to_tcp` to TCP streams.
    fn pipe_out_tcp(&mut self) {
        let l2cap_to_tcp_receive = self.l2cap_to_tcp_receive.clone();
        let port_to_tcp_stream = self.port_to_tcp_stream.clone();
        let handler = tokio::spawn(async move {
            loop {
                let pkt = match Packet::deserialize(l2cap_to_tcp_receive.clone()).await {
                    Ok(pkt) => pkt,
                    Err(e) => {
                        error!("Error deserializing packet; dropping data packet: {e}");
                        continue;
                    }
                };

                match pkt {
                    Packet::Data { port, data } => {
                        if data.len() == 0 {
                            warn!("Empty packet; dropping data packet");
                            continue;
                        }

                        let mut muxed_stream = match port_to_tcp_stream.get_mut(&port) {
                            Some(muxed_stream) => muxed_stream,
                            None => {
                                debug!("Unknown 'port' {port}; dropping data packet");
                                continue;
                            }
                        };

                        if let Err(e) = muxed_stream.writer.write(&data).await {
                            error!(
                                "Error writing to TCP stream for 'port' {port}; dropping data packet: {e}",
                            );
                            continue;
                        }
                    }
                    Packet::Control {
                        msg_type,
                        for_port,
                        status,
                        ..
                    } => {
                        if msg_type == 0 {
                            trace!("Received keepalive control packet");
                            continue;
                        }
                        if msg_type != 0 {
                            error!(
                                "Do not know how to handle MSG_TYPE {msg_type}; dropping control packet"
                            );
                            continue;
                        }

                        match status {
                            0 => {
                                // Closed.
                                if !port_to_tcp_stream.contains_key(&for_port) {
                                    error!("Unknown 'port' {for_port}; dropping control packet");
                                    continue;
                                }

                                // TODO: actually close socket?
                                debug!("Closing socket for port {for_port}");
                                port_to_tcp_stream.remove(&for_port);
                            }
                            1 => {
                                // Open.
                                error!("Cannot accept request to open a TCP stream");
                                continue;
                            }
                            _ => {
                                error!(
                                    "Do not know how to handle control packet FOR_PORT {for_port} STATUS {status}"
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

    /// Reads from `tcp_to_l2cap` into L2CAP stream.
    fn pipe_in_tcp(&mut self, mut l2cap_stream_write: WriteHalf<l2cap::Stream>) {
        let tcp_to_l2cap_receive = self.tcp_to_l2cap_receive.clone();
        let handler = tokio::spawn(async move {
            loop {
                match tcp_to_l2cap_receive.recv().await {
                    Ok(packet) => {
                        let serialized_packet = match packet.serialize().await {
                            Ok(serialized_packet) => serialized_packet,
                            Err(e) => {
                                error!("Error serializing packet; dropping packet: {e}");
                                continue;
                            }
                        };
                        if let Err(e) = l2cap_stream_write.write_all(&serialized_packet).await {
                            error!("Error writing to L2CAP stream; dropping packet: {e}");
                            continue;
                        }
                    }
                    Err(e) => {
                        error!("Error receiving from 'tcp_to_l2cap' channel; likely closed: {e}");
                        break;
                    }
                }
            }
        });
        self.tasks.push(handler);
    }

    /// Sends keepalives.
    fn send_keepalive_frames_forever(&mut self) {
        let tcp_to_l2cap_send = self.tcp_to_l2cap_send.clone();
        let handler = tokio::spawn(async move {
            loop {
                let keepalive_packet = match Packet::keepalive().await {
                    Ok(keepalive_packet) => keepalive_packet,
                    Err(e) => {
                        error!("Could not create keepalive packet: {e}");
                        break;
                    }
                };
                if let Err(e) = tcp_to_l2cap_send.send(keepalive_packet).await {
                    error!("Error sending keepalive to 'tcp_to_l2cap' channel; dropping keep alive: {e}");
                    continue;
                }

                // Sleep for one second between keep alives.
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        });
        self.tasks.push(handler);
    }
}

impl Drop for L2CAPStreamMux {
    fn drop(&mut self) {
        info!("Dropping multiplexer...");
        while let Some(task) = self.tasks.pop() {
            // TODO: more cleanly shut down tasks with a cancelation signal instead of abort.
            task.abort();
        }
        info!("Multiplexer dropped");
    }
}

#[derive(Clone, Debug)]
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
    async fn deserialize(l2cap_to_tcp_receive: Arc<Receiver<Vec<u8>>>) -> Result<Self> {
        let data = vec![0u8; 2];
        // TODO(benji): write deserialize.
        Ok(Self::Data { port: 0, data })
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
                // TODO: document seemingly arbitrary data length.
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
    async fn control_socket_closed(for_port: u16) -> Result<Self> {
        let mut raw_data = vec![0u8; 6];
        // TODO: specify endian ordering?
        raw_data.write_u16(0).await?;
        raw_data.write_u8(1).await?;
        raw_data.write_u16(for_port).await?;
        raw_data.write_u8(0).await?;

        Ok(Self::Control {
            msg_type: 1,
            for_port,
            status: 0,
            raw_data,
        })
    }

    /*
    Keep Alive

    +------+----------+
    | PORT | MSG_TYPE |
    +------+----------+
    | 2=0  |  1=0     |
    +------+----------+
    */
    async fn keepalive() -> Result<Self> {
        let mut raw_data = vec![0u8; 3];
        // TODO: specify endian ordering?
        raw_data.write_u16(0).await?;
        raw_data.write_u8(0).await?;

        Ok(Self::Control {
            msg_type: 0,
            for_port: 0,
            status: 0,
            raw_data,
        })
    }
}

/// The WriteHalf of a TCP stream to be multiplexed.
struct MuxedTCPStream {
    // ReadHalf is owned by thread in `add_socket`.
    writer: WriteHalf<TcpStream>,
}
