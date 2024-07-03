//! Connects to l2cap server and sends and receives test data.

use bluer::{
    l2cap::{SocketAddr, Stream},
    Address, AddressType,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const PSM: u16 = 192;

#[tokio::main(flavor = "current_thread")]
async fn main() -> bluer::Result<()> {
    let session = bluer::Session::new().await?;
    let adapter = session.default_adapter().await?;
    adapter.set_powered(true).await?;

    let target_addr: Address = "FILLIN".parse().expect("invalid address");
    let target_sa = SocketAddr::new(target_addr, AddressType::LeRandom, PSM);

    println!("Connecting to {:?}", &target_sa);
    let mut stream = Stream::connect(target_sa).await.expect("connection failed");
    println!("Local address: {:?}", stream.as_ref().local_addr()?);
    println!("Remote address: {:?}", stream.peer_addr()?);
    println!("Send MTU: {:?}", stream.as_ref().send_mtu());
    println!("Recv MTU: {}", stream.as_ref().recv_mtu()?);
    println!("Security: {:?}", stream.as_ref().security()?);
    println!("Flow control: {:?}", stream.as_ref().flow_control());

    println!("\nSending message");
    let my_string = "hello there".to_string();

    // Note that write_all will automatically split the buffer into
    // multiple writes of MTU size.
    stream
        .write_all(my_string.as_bytes())
        .await
        .expect("write failed");

    println!("\nReceiving message");
    let mut message_buf = [0u8; 1024];
    stream.read(&mut message_buf).await.expect("read failed");
    println!("Received: {}", String::from_utf8_lossy(&message_buf));

    println!("Done");
    Ok(())
}
