//! Defines SOCKS logic.

use bluer::l2cap;
use log::info;
use tokio::net::TcpListener;

/// The port on which to start the SOCKS proxy.
const PORT: u16 = 5000;

/// Starts a SOCKS proxy that accepts incoming SOCKS requests and writes them over `stream` using
/// multiplexing.
pub async fn start_proxy(stream: l2cap::Stream) -> bluer::Result<()> {
    let bind_address = format!("127.0.0.1:{PORT}");
    let listener = TcpListener::bind(bind_address.clone()).await?;
    info!("SOCKS proxy now listening on {bind_address}");

    while let Ok((tcp_stream, addr)) = listener.accept().await {}

    Ok(())
}
