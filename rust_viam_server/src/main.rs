//! Scans for BLE device with Viam service UUID and PSM char, and opens L2CAP
//! socket over that advertised PSM.

use bluer::{
    l2cap::{SocketAddr, Stream},
    AdapterEvent, Address, AddressType, Device,
};
use futures::{pin_mut, StreamExt};
use std::time::Duration;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    time::sleep,
};
use uuid::uuid;

/// Service UUID for GATT example.
const VIAM_SERVICE_UUID: uuid::Uuid = uuid!("79cf4eca-116a-4ded-8426-fb83e53bc1d7");

/// Characteristic UUID for GATT example.
const PSM_CHARACTERISTIC_UUID: uuid::Uuid = uuid!("ab76ead2-b6e6-4f12-a053-61cd0eed19f9");

async fn find_address_and_psm(adapter: &bluer::Adapter) -> bluer::Result<(Device, u16)> {
    println!(
        "Discovering on Bluetooth adapter {} with address {}\n",
        adapter.name(),
        adapter.address().await?
    );
    let discover = adapter.discover_devices().await?;

    pin_mut!(discover);
    while let Some(evt) = discover.next().await {
        match evt {
            AdapterEvent::DeviceAdded(addr) => {
                let device = adapter.device(addr)?;
                let addr = device.address();
                let uuids = device.uuids().await?.unwrap_or_default();

                if uuids.contains(&VIAM_SERVICE_UUID) {
                    println!("    Device {addr} provides our service!");

                    sleep(Duration::from_secs(2)).await;
                    if !device.is_connected().await? {
                        println!("    Connecting...");
                        let mut retries = 2;
                        loop {
                            match device.connect().await {
                                Ok(()) => break,
                                Err(err) if retries > 0 => {
                                    println!("    Connect error: {}", &err);
                                    retries -= 1;
                                }
                                Err(err) => return Err(err),
                            }
                        }
                        println!("    Connected");
                    } else {
                        println!("    Already connected");
                    }

                    println!("    Enumerating services...");
                    for service in device.services().await? {
                        let uuid = service.uuid().await?;
                        println!("    Service UUID: {}", &uuid);
                        if uuid == VIAM_SERVICE_UUID {
                            println!("    Found our service!");
                            for char in service.characteristics().await? {
                                let uuid = char.uuid().await?;
                                println!("    Characteristic UUID: {}", &uuid);
                                if uuid == PSM_CHARACTERISTIC_UUID {
                                    println!("    Found our characteristic!");
                                    if char.flags().await?.read {
                                        println!("    Reading characteristic value");
                                        let value = char.read().await?;
                                        println!("    Read value: {:x?}", &value);
                                        let str_psm = String::from_utf8_lossy(&value);
                                        let psm = str_psm
                                            .parse::<u16>()
                                            .expect("Failed to convert psm to u16");
                                        return Ok((device, psm));
                                    }
                                }
                            }
                        }
                    }

                    println!("    Not found!");
                }
            }
            _ => (),
        }
    }
    Err(bluer::Error {
        kind: bluer::ErrorKind::Failed,
        message: "failed".to_string(),
    })
}

async fn run_l2cap(target_addr: Address, psm: u16) -> bluer::Result<()> {
    let target_sa = SocketAddr::new(target_addr, AddressType::LePublic, psm);

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
    stream.shutdown().await.expect("shutdown failed");
    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> bluer::Result<()> {
    let session = bluer::Session::new().await?;
    let adapter = session.default_adapter().await?;
    adapter.set_powered(true).await?;

    let (device, psm) = find_address_and_psm(&adapter)
        .await
        .expect("finding address and psm failed");

    //let events = device.events().await.expect("bad device stream");
    //pin_mut!(events);
    //while let Some(evt) = events.next().await {
    //match evt {
    //DeviceEvent::PropertyChanged(prop) => match prop {
    //DeviceProperty::Address(addr) => {
    //println!("    Address change event detected");
    //}
    //_ => (),
    //},
    //}
    //}

    match device.set_trusted(true).await {
        Ok(()) => println!("    Device trusted"),
        Err(err) => println!("    Device trust failed: {}", &err),
    }

    // Check if device is actually trusted.
    match device.is_trusted().await {
        Ok(trusted) => match trusted {
            true => println!("     Device is actually trusted"),
            false => println!("     Device is NOT actually trusted"),
        },
        Err(err) => println!("    Device get trust failed: {}", &err),
    }

    println!("    Sleeping; Trust the device now!");
    std::thread::sleep(std::time::Duration::from_secs(10));

    run_l2cap(device.address(), psm)
        .await
        .expect("opening l2cap socket failed");

    match device.disconnect().await {
        Ok(()) => println!("    Device disconnected"),
        Err(err) => println!("    Device disconnection failed: {}", &err),
    }
    println!();

    Ok(())
}
