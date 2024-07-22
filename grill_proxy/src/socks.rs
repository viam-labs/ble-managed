//! Defines SOCKS forwarding logic.

use bluer::l2cap;
use log::{debug, info};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

/// The port on which to start the SOCKS proxy.
const PORT: u16 = 5000;

/// Starts a SOCKS proxy that accepts incoming SOCKS requests and forwards them over `stream` using
/// multiplexing.
pub async fn start_proxy(stream: &mut l2cap::Stream) -> bluer::Result<()> {
    let bind_address = format!("127.0.0.1:{PORT}");
    let listener = TcpListener::bind(bind_address.clone()).await?;
    info!("SOCKS proxy now listening on {bind_address}");

    // TODO: Use _addr to multiplex.
    while let Ok((mut tcp_stream, _addr)) = listener.accept().await {
        let mut num_received_msgs = 2;
        loop {
            // Wait for the stream to be readable.
            debug!("Waiting for TCP stream to be readable...");
            tcp_stream.readable().await?;

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
                    return Err(e.into());
                }
            }

            // Note that write_all will automatically split the buffer into
            // multiple writes of MTU size.
            debug!("Forwarding request to L2CAP stream...");
            stream.write_all(&message_buf).await?;

            debug!("Reading response from L2CAP stream...");
            let mtu_as_cap = stream.as_ref().recv_mtu()?;
            let mut message_buf = vec![0u8; mtu_as_cap as usize];
            let n = stream.read(&mut message_buf).await?;
            num_received_msgs += 1;

            message_buf.truncate(n);
            let length = message_buf.len();
            debug!("Writing response to TCP stream: length of message was {length}...");
            if num_received_msgs < 3 {
                // Only print message context for first 2 messages.
                debug!("Writing response to TCP stream: message was {message_buf:#?}...");
            }
            tcp_stream.write_all(&message_buf).await?;
        }
    }

    Ok(())
}
