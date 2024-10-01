//! Defines SOCKS forwarding logic.

mod chunker;
mod mux;

use anyhow::{anyhow, Result};
use bluer::l2cap;
use log::{debug, error, info, warn};
use tokio::net::TcpListener;
use tokio::signal::unix::{signal, SignalKind};

/// The port on which to start the SOCKS proxy.
const PORT: u16 = 1080;

/// Value to set for incoming maximum-transmission-unit on created L2CAP streams.
const RECV_MTU: u16 = 65535;

/// Starts a SOCKS proxy that accepts incoming SOCKS requests and forwards them over streams
/// created against the `device` on `psm`. Returns true if SOCKS proxy should be restarted
/// (start listening for new connections again), and false if not.
pub async fn start_proxy(device: bluer::Device, psm: u16) -> Result<bool> {
    let bind_address = format!("127.0.0.1:{PORT}");
    let listener = TcpListener::bind(bind_address.clone()).await?;

    let l2cap_stream = match connect_l2cap(&device, psm).await {
        Ok(stream) => stream,
        Err(e) => {
            return Err(anyhow!("Error creating L2CAP stream: {e}"));
        }
    };
    let mut mux = mux::L2CAPStreamMux::create_and_start(l2cap_stream);

    info!("SOCKS forwarder now listening on {bind_address}");

    let mut sigterm = signal(SignalKind::terminate())?;
    let mut sigint = signal(SignalKind::interrupt())?;

    loop {
        tokio::select! {
            Ok((tcp_stream, _addr)) = listener.accept() => {
                if let Err(e) = mux.add_tcp_stream(tcp_stream).await {
                    return Err(anyhow!("could not add mux TCP stream: {e}"));
                }
            },
            _ = mux.wait_for_stop_due_to_disconnect() => {
                return Ok(true);
            }
            _ = sigterm.recv() => {
                info!("Stopping SOCKS forwarder (SIGTERM)...");
                break;
            },
            _ = sigint.recv() => {
                info!("Stopping SOCKS forwarder (SIGINT)...");
                break;
            },
        }
    }

    // Disconnect device if still connected after proxy is done running.
    if device.is_connected().await? {
        if let Err(e) = device.disconnect().await {
            warn!("Error disconnecting device (may have already been disconnected): {e}");
        }
    }
    Ok(false)
}

/// Opens a new L2CAP stream to `Device` on `psm`.
pub async fn connect_l2cap(device: &bluer::Device, psm: u16) -> Result<l2cap::Stream> {
    let addr_type = device.address_type().await?;
    let target_sa = l2cap::SocketAddr::new(device.remote_address().await?, addr_type, psm);

    let stream = l2cap::Socket::<l2cap::Stream>::new_stream()?;

    if let Err(e) = stream.set_recv_mtu(RECV_MTU) {
        error!("Error setting recv mtu value of {RECV_MTU}: {e}");
    }

    debug!("Binding socket");
    stream.bind(l2cap::SocketAddr::any_le())?;

    info!("Connecting to L2CAP CoC at {:?}", &target_sa);
    stream
        .connect(target_sa)
        .await
        .map_err(|e| anyhow!("error creating L2CAP stream: {e}"))
}
