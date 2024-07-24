//! Defines SOCKS forwarding logic.

use crate::central;
use log::{debug, error, info};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

/// The port on which to start the SOCKS proxy.
const PORT: u16 = 5000;

/// Starts a SOCKS proxy that accepts incoming SOCKS requests and forwards them over streams
/// created against the `device` on `psm`.
pub async fn start_proxy(device: bluer::Device, psm: u16) -> bluer::Result<()> {
    let bind_address = format!("127.0.0.1:{PORT}");
    let listener = TcpListener::bind(bind_address.clone()).await?;
    info!("SOCKS proxy now listening on {bind_address}");

    // Set flow control.
    // stream.as_ref().set_flow_control(l2cap::FlowControl::Le)?;

    // TODO: Use _addr to multiplex.
    while let Ok((mut tcp_stream, _addr)) = listener.accept().await {
        let mut num_received_msgs = 0;
        let device_clone = device.clone();

        tokio::spawn(async move {
            // Create new stream
            let mut stream = match central::connect_l2cap(device_clone, psm).await {
                Ok(stream) => stream,
                Err(e) => {
                    error!("Error creating L2CAP stream: {e}");
                    return;
                }
            };
            loop {
                // Wait for the stream to be readable.
                debug!("Waiting for TCP stream to be readable...");
                if let Err(e) = tcp_stream.readable().await {
                    error!("Error waiting for tcp stream to be readable: {e}");
                    break;
                }

                // Try to read data, this may still fail with `WouldBlock` if the readiness event is a
                // false positive.
                let mut message_buf = vec![0u8; 1024];
                match tcp_stream.try_read(&mut message_buf) {
                    Ok(n) => {
                        message_buf.truncate(n);
                        debug!("Incoming request from TCP stream: {message_buf:#?}");
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        debug!("Continuing due to possible block...");
                        continue;
                    }
                    Err(e) => {
                        error!("Error reading from TCP stream: {e}");
                        break;
                    }
                }

                // Note that write_all will automatically split the buffer into
                // multiple writes of MTU size.
                debug!("Forwarding request to L2CAP stream...");
                if let Err(e) = stream.write_all(&message_buf).await {
                    error!("Error writing to L2CAP stream: {e}");
                    break;
                }

                let mtu_as_cap = match stream.as_ref().recv_mtu() {
                    Ok(recv_mtu) => recv_mtu,
                    Err(e) => {
                        error!("Error getting recv_mtu from L2CAP stream: {e}");
                        break;
                    }
                };

                loop {
                    debug!("Reading response from L2CAP stream...");
                    let mut message_buf = vec![0u8; mtu_as_cap as usize];
                    let n = match stream.read(&mut message_buf).await {
                        Ok(n) => n,
                        Err(e) => {
                            error!("Error reading from L2CAP stream: {e}");
                            break;
                        }
                    };
                    if n == 0 {
                        // Stop trying to read when we can't read any more bytes.
                        break;
                    }
                    num_received_msgs += 1;

                    message_buf.truncate(n);
                    let length = message_buf.len();
                    debug!("Writing response to TCP stream: length of message was {length}...");
                    if num_received_msgs < 3 {
                        // Only print message context for first 2 messages.
                        debug!("Writing response to TCP stream: message was {message_buf:#?}...");
                    }
                    if let Err(e) = tcp_stream.write_all(&message_buf).await {
                        error!("Error writing to TCP stream: {e}");
                        break;
                    }
                }
            }
        });
    }

    Ok(())
}
