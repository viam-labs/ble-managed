//! Defines SOCKS forwarding logic.

mod mux;

use bluer::l2cap;
use log::{debug, error, info, trace};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

/// The port on which to start the SOCKS proxy.
const PORT: u16 = 5000;

/// Value to set for incoming maximum-transmission-unit on created L2CAP streams.
const RECV_MTU: u16 = 65535;

/// Starts a SOCKS proxy that accepts incoming SOCKS requests and forwards them over streams
/// created against the `device` on `psm`.
pub async fn start_proxy(device: bluer::Device, psm: u16) -> bluer::Result<()> {
    let bind_address = format!("127.0.0.1:{PORT}");
    let listener = TcpListener::bind(bind_address.clone()).await?;
    info!("SOCKS proxy now listening on {bind_address}");

    // TODO: Use _addr to multiplex.
    while let Ok((tcp_stream, _addr)) = listener.accept().await {
        let device_clone = device.clone();

        // Spawn a coroutine to handle incoming connection; continue to listen for more.
        tokio::spawn(async move {
            let (mut tcp_stream_read, mut tcp_stream_write) = tokio::io::split(tcp_stream);

            let l2cap_stream = match connect_l2cap(device_clone, psm).await {
                Ok(stream) => stream,
                Err(e) => {
                    error!("Error creating L2CAP stream: {e}");
                    return;
                }
            };
            let (mut l2cap_stream_read, mut l2cap_stream_write) = tokio::io::split(l2cap_stream);

            // Spawn a coroutine to read from L2CAP stream and write to TCP stream.
            tokio::spawn(async move {
                loop {
                    debug!("Reading response from L2CAP stream...");
                    let mut message_buf = vec![0u8; RECV_MTU as usize];
                    let n = match l2cap_stream_read.read(&mut message_buf).await {
                        Ok(n) if n > 0 => n,
                        Ok(_) => {
                            debug!("L2CAP stream closed");
                            break;
                        }
                        Err(e) => {
                            error!("Error reading from L2CAP stream: {e}");
                            break;
                        }
                    };

                    message_buf.truncate(n);
                    let length = message_buf.len();
                    debug!("Writing response of length {length} to TCP stream...");
                    trace!("Response message was {message_buf:#?}");

                    if let Err(e) = tcp_stream_write.write_all(&message_buf).await {
                        error!("Error writing to TCP stream: {e}");
                        break;
                    }
                }
            });

            // Spawn a coroutine to read from TCP stream and write to L2CAP stream.
            tokio::spawn(async move {
                loop {
                    debug!("Reading request from TCP stream...");
                    let mut message_buf = vec![0u8; 1024]; // TODO(better cap here);
                    let n = match tcp_stream_read.read(&mut message_buf).await {
                        Ok(n) if n > 0 => n,
                        Ok(_) => {
                            debug!("TCP stream closed");
                            break;
                        }
                        Err(e) => {
                            error!("Error reading from TCP stream: {e}");
                            break;
                        }
                    };

                    message_buf.truncate(n);
                    let length = message_buf.len();
                    debug!("Writing request of length {length} to L2CAP stream...");
                    trace!("Request message was {message_buf:#?}");

                    // Note that write_all will automatically split the buffer into multiple writes
                    // of MTU size.
                    if let Err(e) = l2cap_stream_write.write_all(&message_buf).await {
                        error!("Error writing to L2CAP stream: {e}");
                        break;
                    }
                }
            });
        });
    }

    Ok(())
}

/// Opens a new L2CAP stream to `Device` on `psm`.
pub async fn connect_l2cap(device: bluer::Device, psm: u16) -> bluer::Result<l2cap::Stream> {
    let addr_type = device.address_type().await?;
    let target_sa = l2cap::SocketAddr::new(device.remote_address().await?, addr_type, psm);

    let stream = l2cap::Socket::<l2cap::Stream>::new_stream()?;

    if let Err(e) = stream.set_recv_mtu(RECV_MTU) {
        error!("Error setting recv mtu value of {RECV_MTU}: {e}");
    }

    debug!("Binding socket");
    stream.bind(l2cap::SocketAddr::any_le())?;

    info!("Connecting to L2CAP CoC at {:?}", &target_sa);
    stream.connect(target_sa).await.map_err(|e| bluer::Error {
        kind: bluer::ErrorKind::Failed,
        message: format!("{e}"),
    })
}
