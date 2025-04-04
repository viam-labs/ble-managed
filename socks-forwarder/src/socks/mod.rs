//! Defines SOCKS forwarding logic.

mod chunker;
mod mux;

use anyhow::{anyhow, Result};
use bluer::l2cap;
use log::{debug, error, info, warn};
use tokio::net::TcpListener;
use tokio::signal::unix::{signal, SignalKind};
use tokio::time::{self, timeout, Duration};

/// The port on which to start the listening for traffic to forward.
const PORT: u16 = 1080;

/// Value to set for incoming maximum-transmission-unit on created L2CAP streams.
const RECV_MTU: u16 = 65535;

/// Starts a forwarder that accepts incoming requests and forwards them over an L2CAP stream
/// created against the `device` on `psm`. Returns true if main program should go back to
/// `find_viam_mobile_device_and_psm` and false otherwise (only returns false when a SIGTERM or
/// SIGINT is received.)
pub async fn start_forwarder(device: bluer::Device, psm: u16) -> Result<bool> {
    let bind_address = format!("127.0.0.1:{PORT}");
    let listener = TcpListener::bind(bind_address.clone()).await?;

    let l2cap_stream = match connect_l2cap(&device, psm).await {
        Ok(stream) => stream,
        Err(e) => {
            return Err(anyhow!("Error creating L2CAP stream: {e}"));
        }
    };
    let mut mux = mux::L2CAPStreamMux::create_and_start(l2cap_stream);

    info!("BLE-SOCKS bridge established and ready to handle traffic");

    let mut sigterm = signal(SignalKind::terminate())?;
    let mut sigint = signal(SignalKind::interrupt())?;

    let mut should_restart_main_program = true;
    loop {
        tokio::select! {
            Ok((tcp_stream, _addr)) = listener.accept() => {
                if let Err(e) = mux.add_tcp_stream(tcp_stream).await {
                    return Err(anyhow!("could not add mux TCP stream: {e}"));
                }
            },
            _ = mux.wait_for_stop_due_to_disconnect() => {
                break;
            }
            _ = sigterm.recv() => {
                info!("Received SIGTERM signal while handling traffic; stopping the SOCKS forwarder");
                should_restart_main_program = false;
                break;
            },
            _ = sigint.recv() => {
                info!("Received SIGINT signal while handling; stopping the SOCKS forwarder");
                should_restart_main_program = false;
                break;
            }
        }
    }

    debug!("Sleeping for a couple seconds to potentially allow manual disconnect");
    time::sleep(Duration::from_secs(2)).await;

    // Disconnect device if still connected after forwarder is done running.
    if device.is_connected().await? {
        let disconnect_future = device.disconnect();
        let disconnect_timeout = Duration::from_secs(5);

        match timeout(disconnect_timeout, disconnect_future).await {
            Ok(result) => {
                if let Err(e) = result {
                    warn!("Error disconnecting device (may have already been disconnected): {e}");
                } else {
                    info!("Disconnected from remote device");
                }
            }
            Err(_) => {
                warn!("Failed to disconnect from remote device after {disconnect_timeout:?}");
            }
        }
    }
    Ok(should_restart_main_program)
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

    debug!("Setting security level to high");
    let security = l2cap::Security {
        level: l2cap::SecurityLevel::High,
        key_size: 16,
    };
    stream.set_security(security)?;

    info!("Connecting to L2CAP CoC at {:?}", &target_sa);
    stream
        .connect(target_sa)
        .await
        .map_err(|e| anyhow!("error creating L2CAP stream: {e}"))
}
