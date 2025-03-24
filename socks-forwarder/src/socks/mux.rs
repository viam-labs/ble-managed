//! Implements one side of the multiplexing protocol defined in the following specification.
//! https://github.com/viamrobotics/flutter-ble/blob/bbe7e2a511c452f932c52e3784d7dca3751a03bd/doc/sockets.md

use std::{
    io::Write, sync::{
        atomic::{AtomicU16, Ordering::Relaxed},
        Arc,
    }, time::Instant
};

use super::chunker::Chunker;

use anyhow::{anyhow, Result};
use async_channel::{self, Receiver, Sender};
use bluer::l2cap;
use byteorder::{ByteOrder, LittleEndian, WriteBytesExt};
use dashmap::DashMap;
use log::{debug, error, info, trace, warn};
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
    // `Packet`s from TCP streams to send to L2CAP stream. Stored as an `Arc` on struct as it is
    // used in multiple places (`add_tcp_stream` and `send_keepalive_frames_forever`).
    tcp_to_l2cap_send: Arc<Sender<Packet>>,
    // Group of tasks.
    tasks: Vec<JoinHandle<()>>,
    // Stopped or not (mux can be stopped when L2CAP is disconnected or when mux is dropped).
    stopped: bool,
    // Channel to send stop requests due to L2CAP disconnection.
    stop_due_to_disconnect_send: Arc<Sender<bool>>,
    // Channel to receive stop requests due to L2CAP disconnection.
    stop_due_to_disconnect_receive: Receiver<bool>,
}

impl L2CAPStreamMux {
    /// Creates new mux from an L2CAP stream.
    pub(crate) fn create_and_start(stream: l2cap::Stream, speed_test_mode: bool) -> Self {
        info!("Starting L2CAP stream multiplexer...");
        let next_port = AtomicU16::new(1); // Start at 1 to distinguish between control packets.
        let port_to_tcp_stream = Arc::new(DashMap::default());

        let tasks = Vec::new();

        let (tcp_to_l2cap_send, tcp_to_l2cap_receive) = async_channel::unbounded::<Packet>();
        let (stop_due_to_disconnect_send, stop_due_to_disconnect_receive) =
            async_channel::bounded::<bool>(1);

        let mut mux = L2CAPStreamMux {
            next_port,
            port_to_tcp_stream,
            tcp_to_l2cap_send: Arc::new(tcp_to_l2cap_send),
            tasks,
            stopped: false,
            stop_due_to_disconnect_send: Arc::new(stop_due_to_disconnect_send),
            stop_due_to_disconnect_receive,
        };

        // Before splitting stream into read and write halves, log MTUs.
        mux.log_mtus(stream.as_ref());

        let (l2cap_stream_read, l2cap_stream_write) = tokio::io::split(stream);
        let (l2cap_to_tcp_send, l2cap_to_tcp_receive) = async_channel::unbounded::<Vec<u8>>();

        // default behavior is to not be in speed_test_mode, so evaluate that first
        if !speed_test_mode {
            mux.pipe_in_l2cap(l2cap_stream_read, l2cap_to_tcp_send);
            mux.pipe_out_tcp(Chunker::new(l2cap_to_tcp_receive));
            mux.pipe_in_tcp(l2cap_stream_write, tcp_to_l2cap_receive);
            mux.send_keepalive_frames_forever(); 
        } else {
            mux.speed_test(l2cap_stream_write, l2cap_stream_read);
        }

        info!("Started L2CAP stream multiplexer");
        mux
    }

    /// Logs (at debug level) the current sending and receiving MTU values.
    fn log_mtus(&self, socket: &l2cap::Socket<l2cap::Stream>) {
        match socket.send_mtu() {
            Ok(smtu) => {
                debug!("Sending MTU on the connection will be {smtu}");
            }
            Err(e) => {
                debug!("Could not calculate sending MTU; likely not yet negotiated. Error was {e}");
            }
        };
        match socket.recv_mtu() {
            Ok(rmtu) => {
                debug!("Receiving MTU on the connection will be {rmtu}");
            }
            Err(e) => {
                debug!(
                    "Could not calculate receiving MTU; likely not yet negotiated. Error was {e}"
                );
            }
        };
    }

    /// Incorporates a new TCP stream into the multiplexer.
    pub(crate) async fn add_tcp_stream(&mut self, stream: TcpStream) -> Result<()> {
        debug!("Adding new TCP stream to multiplexer...");

        // Get new "port" value from atomic (start at 1 if overflow).
        if self.next_port.load(Relaxed) > 65534 {
            self.next_port.store(1, Relaxed);
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
        let control_packet = Packet::control_socket_open(port)?;
        self.tcp_to_l2cap_send.send(control_packet).await?;

        let tcp_to_l2cap_send = self.tcp_to_l2cap_send.clone();
        // Spawn coroutine (and track it) to continue reading from TCP stream and writing to
        // 'tcp_to_l2cap' channel.
        let handler = tokio::spawn(async move {
            loop {
                // TODO: use a non-arbitrary cap here.
                let mut data = vec![0u8; 1024];
                let n = match tcp_stream_read.read(&mut data).await {
                    Ok(n) if n > 0 => n,
                    Ok(_) => {
                        debug!("TCP stream closed for 'port' {port}");
                        // Send a close control packet.
                        let control_packet = match Packet::control_socket_closed(port) {
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
                        info!("Could not read from TCP stream (likely closed); closing for 'port' {port}: {e}");
                        // Send a close control packet.
                        let control_packet = match Packet::control_socket_closed(port) {
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
                data.truncate(n);
                debug!(
                    "Writing data packet for 'port' {port} from TCP stream of length {}...",
                    data.len()
                );
                trace!("Data in packet to be written is {data:#?}");

                let data_packet = Packet::Data { port, data };
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

    /// Reads from `l2cap_stream_read` into `l2cap_to_tcp`.
    fn pipe_in_l2cap(
        &mut self,
        mut l2cap_stream_read: ReadHalf<l2cap::Stream>,
        l2cap_to_tcp_send: Sender<Vec<u8>>,
    ) {
        let handler = tokio::spawn(async move {
            loop {
                let mut chunk_buf = vec![0u8; RECV_MTU as usize];
                let n = match l2cap_stream_read.read(&mut chunk_buf).await {
                    Ok(n) if n > 0 => n,
                    Ok(_) => {
                        info!("L2CAP stream closed");
                        break;
                    }
                    Err(e) => {
                        warn!("Error reading from L2CAP stream: {e}");
                        break;
                    }
                };
                chunk_buf.truncate(n);

                if let Err(e) = l2cap_to_tcp_send.send(chunk_buf).await {
                    error!("Error sending to 'l2cap_to_tcp' channel; dropping chunk: {e}");
                    continue;
                }
            }
        });
        self.tasks.push(handler);
    }

    /// Reads from `l2cap_to_tcp_chunker` to TCP streams.
    fn pipe_out_tcp(&mut self, mut l2cap_to_tcp_chunker: Chunker) {
        let port_to_tcp_stream = self.port_to_tcp_stream.clone();
        let stop_due_to_disconnect_send = self.stop_due_to_disconnect_send.clone();
        let handler = tokio::spawn(async move {
            loop {
                let pkt = match Packet::deserialize(&mut l2cap_to_tcp_chunker).await {
                    Ok(pkt) => pkt,
                    Err(e) => {
                        // Inability to deserialize a packet indicates degradation or disconnection
                        // of the L2CAP connection; send to stop_due_to_disconnect channel.
                        warn!("Error deserializing packet; dropping data packet: {e}");
                        if let Err(e) = stop_due_to_disconnect_send.send(true).await {
                            error!("Error sending to 'stop_due_to_disconnect' channel: {e}");
                        }
                        break;
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

                        debug!(
                            "Received data packet for 'port' {port} from L2CAP stream of length {}...",
                            data.len()
                        );
                        trace!("Data in received packet is {data:#?}");

                        if let Err(e) = muxed_stream.writer.write(&data).await {
                            info!(
                                "Could not write to TCP stream for 'port' {port} (stream may be closed); dropping data packet: {e}",
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
                        if msg_type != 1 {
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

    // The structure here should match the corresponding _uploadTest in the phone proxy app
    async fn upload_test(
        mut l2cap_stream_write: WriteHalf<l2cap::Stream>,
    ) {
        let bytes_per_test = 200000 as f64;
        info!("Starting upload speed test!");
        let mut test_num = 1;
        let num_tests = 2;

        let mut total_sent: f64 = 0.0;
        let mut total_elapsed: f64 = 0.0;
        loop {
            let mut total = 0;
            let mut msg_num = 0;
            let start = Instant::now();
            loop {
                // for whatever reason, 29000 bytes seems to be the largest number that works reliably on test device (Pixel 7 on Android 15).
                // using 25000 because that seems to average the highest speed.
                // feel free to increase or decrease this. 
                const BYTES_PER_WRITE: usize = 25000;
                let a = [(); BYTES_PER_WRITE].map(|_| msg_num);
                if let Err(e) = l2cap_stream_write.write_all(&a).await {
                    error!("Error writing to L2CAP stream; ending network test: {e}");
                    return
                }
                total += a.len();

                if total >= bytes_per_test as usize {
                    let mb_sent = total as f64/1000000.;
                    let elapsed_time = start.elapsed().as_millis() as f64/1000.;

                    let mut test_log = String::new();
                    test_log.push_str("\n");
                    test_log.push_str(&format!("Test #{} of {}\n", test_num, num_tests));
                    test_log.push_str(&format!("\tData sent: {:.3} megabytes\n", mb_sent));
                    test_log.push_str(&format!("\tTime elapsed: {:.3}s\n", elapsed_time));
                    test_log.push_str(&format!("\tUpload Speed: {:.3} megabytes/s ({:.3} megabits/s)\n", mb_sent/elapsed_time, 8.*mb_sent/elapsed_time));
                    info!("{}", test_log);

                    total_sent += mb_sent;
                    total_elapsed += elapsed_time;
                    // await to make sure the other side has fully received data
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    break
                }
                msg_num += 1;
            }
            test_num += 1;
            if test_num > num_tests {
                let mut test_log = String::new();
                test_log.push_str("\n");
                test_log.push_str("Upload Speed Test Summary:\n");
                test_log.push_str(&format!("\tTotal sent: {:.3} megabytes\n", total_sent));
                test_log.push_str(&format!("\tTotal elapsed: {:.3}s\n", total_elapsed));
                test_log.push_str(&format!("\tAvg Upload Speed: {:.3} megabytes/s ({:.3} megabits/s)\n", total_sent/total_elapsed, 8.*total_sent/total_elapsed));
                info!("{}", test_log);
                break
            }
        }
    }

    async fn download_test(
        mut l2cap_stream_read: ReadHalf<l2cap::Stream>,
    ) {
        let bytes_per_test = 200000 as f64;
        info!("Starting download speed test!");
        let mut test_num = 1;
        let num_tests = 5;

        let mut total_recv: f64 = 0.0;
        let mut total_elapsed: f64 = 0.0;
        loop {
            let mut total = 0;
            let mut msg_num = 0;
            let start = Instant::now();
            loop {
                // for whatever reason, 29000 bytes seems to be the largest number that works reliably on test device (Pixel 7 on Android 15).
                // using 25000 because that seems to average the highest speed.
                // feel free to increase or decrease this. 
                const BYTES_PER_WRITE: usize = 25000;
                let mut chunk_buf = vec![0u8; RECV_MTU as usize];
                let n = match l2cap_stream_read.read(&mut chunk_buf).await {
                    Ok(n) if n > 0 => n,
                    Ok(_) => {
                        info!("L2CAP stream closed");
                        break;
                    }
                    Err(e) => {
                        warn!("Error reading from L2CAP stream: {e}");
                        break;
                    }
                };
                chunk_buf.truncate(n);
                total += n;

                if total >= bytes_per_test as usize {
                    let mb_recv = total as f64/1000000.;
                    let elapsed_time = start.elapsed().as_millis() as f64/1000.;

                    let mut test_log = String::new();
                    test_log.push_str("\n");
                    test_log.push_str(&format!("Test #{} of {}\n", test_num, num_tests));
                    test_log.push_str(&format!("\tData received: {:.3} megabytes\n", mb_recv));
                    test_log.push_str(&format!("\tTime elapsed: {:.3}s\n", elapsed_time));
                    test_log.push_str(&format!("\tDownload Speed: {:.3} megabytes/s ({:.3} megabits/s)\n", mb_recv/elapsed_time, 8.*mb_recv/elapsed_time));
                    info!("{}", test_log);

                    total_recv += mb_recv;
                    total_elapsed += elapsed_time;
                    break
                }
                msg_num += 1;
            }
            test_num += 1;
            if test_num > num_tests {
                let mut test_log = String::new();
                test_log.push_str("\n");
                test_log.push_str("Download Speed Test Summary:\n");
                test_log.push_str(&format!("\tTotal received: {:.3} megabytes\n", total_recv));
                test_log.push_str(&format!("\tTotal elapsed: {:.3}s\n", total_elapsed));
                test_log.push_str(&format!("\tAvg Download Speed: {:.3} megabytes/s ({:.3} megabits/s)\n", total_recv/total_elapsed, 8.*total_recv/total_elapsed));
                info!("{}", test_log);
                break
            }
        }
    }

    fn speed_test(
        &mut self,
        mut l2cap_stream_write: WriteHalf<l2cap::Stream>,
        mut l2cap_stream_read: ReadHalf<l2cap::Stream>,
    ){
        let stop_due_to_disconnect_send: Arc<Sender<bool>> = self.stop_due_to_disconnect_send.clone();
        let handler = tokio::spawn(async move {
            Self::upload_test(l2cap_stream_write).await;
            Self::download_test(l2cap_stream_read).await;
            // disconnect
            if let Err(e) = stop_due_to_disconnect_send.send(true).await {
                error!("Error sending to 'stop_due_to_disconnect' channel: {e}");
            }
        });
        self.tasks.push(handler);
    }
    
    /// Reads from `tcp_to_l2cap_receive` into `l2cap_stream_write`.
    fn pipe_in_tcp(
        &mut self,
        mut l2cap_stream_write: WriteHalf<l2cap::Stream>,
        tcp_to_l2cap_receive: Receiver<Packet>,
    ) {
        let handler = tokio::spawn(async move {
            loop {
                match tcp_to_l2cap_receive.recv().await {
                    Ok(packet) => {
                        let serialized_packet = match packet.serialize() {
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
                let keepalive_packet = match Packet::keepalive() {
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

    /// Waits for a signal due to L2CAP disconnection and `stop`s the mux if it receives
    /// one.
    pub(crate) async fn wait_for_stop_due_to_disconnect(&mut self) {
        match self.stop_due_to_disconnect_receive.recv().await {
            Ok(_) => {
                warn!("L2CAP disconnection detected");
                self.stop();
            }
            Err(e) => {
                error!("Error receiving from 'stop_due_to_disconnect_receive' channel: {e}");
            }
        }
    }

    /// Idempotently stops the mux.
    fn stop(&mut self) {
        if !self.stopped {
            info!("Stopping multiplexer...");
            while let Some(task) = self.tasks.pop() {
                task.abort();
            }
            self.stopped = true;
            info!("Multiplexer stopped");
        }
    }
}

impl Drop for L2CAPStreamMux {
    fn drop(&mut self) {
        self.stop();
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
    async fn deserialize(l2cap_to_tcp_chunker: &mut Chunker) -> Result<Self> {
        let port_bytes = match l2cap_to_tcp_chunker.read(2).await {
            Ok(port_bytes) => port_bytes,
            Err(e) => {
                return Err(anyhow!("failed to read 2 bytes for 'port': {e}"));
            }
        };
        let port = LittleEndian::read_u16(&port_bytes);

        // Control packet.
        if port == 0 {
            let msg_type_byte = match l2cap_to_tcp_chunker.read(1).await {
                Ok(port_bytes) => port_bytes,
                Err(e) => {
                    return Err(anyhow!("failed to read 1 byte for 'msg_type': {e}"));
                }
            };
            let msg_type = msg_type_byte[0];
            if msg_type == 0 {
                return Ok(Self::keepalive()?);
            }
            if msg_type != 1 {
                return Err(anyhow!("do not know how to handle 'msg_type' {msg_type}"));
            }

            let for_port_bytes = match l2cap_to_tcp_chunker.read(2).await {
                Ok(port_bytes) => port_bytes,
                Err(e) => {
                    return Err(anyhow!("failed to read 2 bytes for 'for_port': {e}"));
                }
            };
            let for_port = LittleEndian::read_u16(&for_port_bytes);

            let status_byte = match l2cap_to_tcp_chunker.read(1).await {
                Ok(port_bytes) => port_bytes,
                Err(e) => {
                    return Err(anyhow!("failed to read 1 byte for 'status': {e}"));
                }
            };
            let status = status_byte[0];

            match status {
                0 => {
                    return Ok(Self::control_socket_closed(for_port)?);
                }
                1 => {
                    error!("Did not expect to receive an 'open' request from this side");
                    return Ok(Self::control_socket_open(for_port)?);
                }
                _ => {
                    return Err(anyhow!(
                        "Do not know how to handle 'for_port' {for_port} and 'status' {status}"
                    ));
                }
            }
        }

        // Data packet.
        let length_bytes = match l2cap_to_tcp_chunker.read(4).await {
            Ok(port_bytes) => port_bytes,
            Err(e) => {
                return Err(anyhow!("failed to read 4 bytes for length: {e}"));
            }
        };
        let length = LittleEndian::read_u32(&length_bytes);

        if length == 0 {
            return Ok(Self::Data {
                port,
                data: vec![0u8, 0],
            });
        }

        let data = match l2cap_to_tcp_chunker.read(length as usize).await {
            Ok(port_bytes) => port_bytes,
            Err(e) => {
                return Err(anyhow!("failed to read {length} bytes for data: {e}"));
            }
        };
        Ok(Self::Data { port, data })
    }

    /*
    +------+-----+------+
    | PORT | LEN | DATA |
    +------+-----+------+
    |   2  |  4  | LEN  |
    +------+-----+------+
    */
    fn serialize(&self) -> Result<Vec<u8>> {
        let data = match self {
            Packet::Data { port, data } => {
                let data_length = data.len();
                // TODO: document seemingly arbitrary data length.
                if data_length > 4294967295 {
                    return Err(anyhow!("data too large to send {}", data_length));
                }

                let mut length_and_data = Vec::new();
                WriteBytesExt::write_u16::<LittleEndian>(&mut length_and_data, port.to_owned())?;
                WriteBytesExt::write_u32::<LittleEndian>(&mut length_and_data, data_length as u32)?;
                Write::write_all(&mut length_and_data, data)?;
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
    fn control_socket_open(for_port: u16) -> Result<Self> {
        let mut raw_data = Vec::new();
        WriteBytesExt::write_u16::<LittleEndian>(&mut raw_data, 0)?;
        WriteBytesExt::write_u8(&mut raw_data, 1)?;
        WriteBytesExt::write_u16::<LittleEndian>(&mut raw_data, for_port)?;
        WriteBytesExt::write_u8(&mut raw_data, 1)?;

        Ok(Self::Control {
            msg_type: 1,
            for_port,
            status: 1,
            raw_data,
        })
    }
    fn control_socket_closed(for_port: u16) -> Result<Self> {
        let mut raw_data = Vec::new();
        WriteBytesExt::write_u16::<LittleEndian>(&mut raw_data, 0)?;
        WriteBytesExt::write_u8(&mut raw_data, 1)?;
        WriteBytesExt::write_u16::<LittleEndian>(&mut raw_data, for_port)?;
        WriteBytesExt::write_u8(&mut raw_data, 0)?;

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
    fn keepalive() -> Result<Self> {
        let mut raw_data = Vec::new();
        WriteBytesExt::write_u16::<LittleEndian>(&mut raw_data, 0)?;
        WriteBytesExt::write_u8(&mut raw_data, 0)?;

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
