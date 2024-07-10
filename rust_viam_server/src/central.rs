//! Defines central logic.

use bluer::{
    l2cap::{SocketAddr, Stream},
    Device,
};
use futures::pin_mut;
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
    // Call discover_devices to resolve remote names.
    let _discover = adapter.discover_devices().await?;
    pin_mut!(_discover);

    for addr in adapter.device_addresses().await? {
        debug!("Looping through {addr}");
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
                                        device.disconnect().await?;
                                        return Ok((device, psm));
                                    }
                                    Err(e) => {
                                        return Err(bluer::Error {
                                            kind: bluer::ErrorKind::Failed,
                                            message: format!("Found PSM is not a valid u16: {e}"),
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
    let mut message_buf = [0u8; 1024];
    stream.read(&mut message_buf).await.expect("read failed");
    Ok(format!("{}", String::from_utf8_lossy(&message_buf)))
}
