//! Defines central logic.

use bluer::{
    l2cap::{SocketAddr, Stream},
    AdapterEvent, Device,
};
use futures::{pin_mut, StreamExt};
use log::{debug, error, info};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use uuid::Uuid;

/// Finds previously paired device and its exposed PSM:
///
/// - with adapter `adapter`
/// - named `device_name`
/// - with a service IDed as `svc_uuuid`
/// - with a characteristic IDed as `psm_char_uuid`
pub async fn find_device_and_psm(
    adapter: &bluer::Adapter,
    device_name: String,
    svc_uuid: Uuid,
    psm_char_uuid: Uuid,
) -> bluer::Result<(Device, u16)> {
    info!(
        "Discovering on Bluetooth adapter {} with address {}\n",
        adapter.name(),
        adapter.address().await?
    );
    let discover = adapter.discover_devices().await?;
    pin_mut!(discover);

    // TODO(high): Ensure this loop will discover the mobile device. After the previous GATT
    // interaction/pairing, the rock4 sometimes never gets a `DeviceAdded` event for the mobile
    // device. We may have to check `adapter.device_addresses()` to get a list of known device
    // addresses, and `adapter.device(addr)` on each to find which one represents `device_name`.
    // I tried that, but it was then sometimes the case that the Viam service could not be found
    // on the known device.
    while let Some(evt) = discover.next().await {
        match evt {
            AdapterEvent::DeviceAdded(addr) => {
                let device = adapter.device(addr)?;
                let addr = device.address();
                let uuids = device.uuids().await?.unwrap_or_default();

                // If device is named, do not check for service UUID unless it matches name written
                // to previously advertised characteristic.
                if let Some(name) = device.name().await? {
                    if name != device_name {
                        continue;
                    }
                }

                if uuids.contains(&svc_uuid) {
                    info!("Device {addr} provides target service");
                    if !device.is_connected().await? {
                        info!("Connecting to {addr}...");
                        // TODO(low): Make this a little more resilient. I use 3 retries because I
                        // often get a "Software caused connection abort" error below once or even
                        // twice in a row. I wish I knew what that error was, and if it represented
                        // something I have incorrectly set up.
                        let mut retries = 3;
                        loop {
                            match device.connect().await {
                                Ok(()) => break,
                                Err(err) if retries > 0 => {
                                    error!("Connect error: {}", &err);
                                    retries -= 1;
                                }
                                Err(err) => return Err(err),
                            }
                        }
                        info!("Connected");
                    } else {
                        debug!("Already connected");
                    }

                    debug!("Enumerating services...");
                    for service in device.services().await? {
                        let uuid = service.uuid().await?;
                        debug!("Service UUID: {}", &uuid);
                        if uuid == svc_uuid {
                            info!("Found target service");
                            for char in service.characteristics().await? {
                                let uuid = char.uuid().await?;
                                debug!("Characteristic UUID: {}", &uuid);
                                if uuid == psm_char_uuid {
                                    info!("Found target characteristic");
                                    if char.flags().await?.read {
                                        debug!("Reading characteristic value");
                                        let value = char.read().await?;
                                        debug!("Read value: {:x?}", &value);
                                        let str_psm = String::from_utf8_lossy(&value);
                                        match str_psm.parse::<u16>() {
                                            Ok(psm) => {
                                                device.set_trusted(true).await?;
                                                // Disconnect before sending back to L2CAP layer,
                                                // as that will create another connection.
                                                //
                                                // TODO(low): Remove this disconnect if possible.
                                                // This one actually might be necessary, though.
                                                device.disconnect().await?;
                                                return Ok((device, psm));
                                            }
                                            Err(e) => {
                                                return Err(bluer::Error {
                                                    kind: bluer::ErrorKind::Failed,
                                                    message: format!(
                                                        "Found PSM is not a valid u16: {e}"
                                                    ),
                                                });
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            _ => (), // Ignore all events beyond AddedDevice.
        }
    }
    Err(bluer::Error {
        kind: bluer::ErrorKind::Failed,
        message: "Service and characteristic combination not found".to_string(),
    })
}

/// Opens a new L2CAP connection to `Device` on `psm`.
pub async fn connect_l2cap(device: &Device, psm: u16) -> bluer::Result<Stream> {
    let addr_type = device.address_type().await?;
    let target_sa = SocketAddr::new(device.address(), addr_type, psm);

    debug!("Connecting to {:?}", &target_sa);
    let stream = Stream::connect(target_sa).await?;

    debug!("Local address: {:?}", stream.as_ref().local_addr()?);
    debug!("Remote address: {:?}", stream.peer_addr()?);
    debug!("Send MTU: {:?}", stream.as_ref().send_mtu());
    debug!("Recv MTU: {}", stream.as_ref().recv_mtu()?);
    debug!("Security: {:?}", stream.as_ref().security()?);

    // TODO(high): Accessing the flow control mode often results in an error (it will just get
    // printed due to lack of `?`); figure out why. I have not tested writing multiple messages,
    // but I sense we may "run out of credits" as we did with the C code. I can see that
    // l2cap_core.c and l2cap_sock.c are creating _basic_ PDUs, which is wrong. Try
    // `stream.as_ref().set_flow_control(FlowControl::Le)`.
    debug!("Flow control: {:?}", stream.as_ref().flow_control());

    Ok(stream)
}

/// Writes `message` to `Stream`.
pub async fn write_l2cap(message: String, stream: &mut Stream) -> bluer::Result<()> {
    // Note that write_all will automatically split the buffer into
    // multiple writes of MTU size.
    stream
        .write_all(message.as_bytes())
        .await
        .map_err(|e| bluer::Error {
            kind: bluer::ErrorKind::Failed,
            message: format!("Failed to write: {e}"),
        })
}

/// Reads a string message from `Stream`.
pub async fn read_l2cap(stream: &mut Stream) -> bluer::Result<String> {
    // TODO(low): Create buffer with incoming MTU as the capacity.
    let mut message_buf = [0u8; 1024];
    stream.read(&mut message_buf).await.expect("read failed");
    Ok(format!("{}", String::from_utf8_lossy(&message_buf)))
}
