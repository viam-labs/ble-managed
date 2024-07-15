//! Defines central logic.

use std::{collections::HashSet, time::Duration};

use bluer::{
    gatt,
    l2cap::{SocketAddr, Stream},
    AdapterEvent, Device, DiscoveryFilter, DiscoveryTransport,
};
use futures::{pin_mut, StreamExt, TryFutureExt};
use log::{debug, info};
use tokio::time::timeout;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    time::sleep,
};

/// Finds previously paired device and its exposed PSM:
///
/// - with adapter `adapter`
/// - named `device_name`
/// - with a service IDed as `svc_uuuid`
/// - with a characteristic IDed as `psm_char_uuid`
pub async fn find_device_and_psm(
    adapter: &bluer::Adapter,
    device_name: String,
    svc_uuid: uuid::Uuid,
    proxy_name_char_uuid: uuid::Uuid,
    psm_char_uuid: uuid::Uuid,
) -> bluer::Result<(Device, u16)> {
    info!(
        "Discovering on Bluetooth adapter {} with address {}\n",
        adapter.name(),
        adapter.address().await?
    );

    let filter = DiscoveryFilter {
        discoverable: false,
        transport: DiscoveryTransport::Le,
        uuids: HashSet::from([svc_uuid]),
        ..Default::default()
    };
    adapter.set_discovery_filter(filter).await?;

    if adapter.is_discovering().await? {
        return Err(bluer::Error {
            kind: bluer::ErrorKind::Failed,
            message: "Must stop discovering outside of this process".to_string(),
        });
    }

    debug!("start discover");

    let discover = adapter.discover_devices_with_changes().await?;
    pin_mut!(discover);

    'evt_loop: while let Some(evt) = discover.next().await {
        match evt {
            AdapterEvent::DeviceAdded(addr) => {
                let device = adapter.device(addr)?;
                let remote_addr = device.remote_address().await?;

                match device.rssi().await? {
                    Some(rssi) if rssi <= -100 => {
                        debug!("Device {remote_addr} out of range; skipping");
                        continue;
                    }
                    None if !device.is_connected().await? => {
                        debug!("Device {remote_addr} has no RSSI and not connected; skipping");
                        continue;
                    }
                    _ => {}
                }

                info!(
                    "Device {remote_addr} connected={} paired={} trusted={}",
                    device.is_connected().await?,
                    device.is_paired().await?,
                    device.is_trusted().await?,
                );

                let uuids = device.uuids().await?.unwrap_or_default();

                if uuids.contains(&svc_uuid) {
                    info!(
                        "Device {remote_addr} provides target service {}",
                        device.address_type().await?
                    );

                    info!("Connecting to {remote_addr}...");
                    let max_retries = 5;
                    let connect_timeout = Duration::from_secs(20);
                    let wait_interval = Duration::from_secs(5);
                    let mut retries = max_retries;

                    let services = loop {
                        match timeout(connect_timeout, connect(&device)).await {
                            Ok(Ok(services)) => break services,
                            Ok(Err(err)) => {
                                debug!("Connect failed: {}", &err);
                            }
                            Err(_) => {
                                debug!("Connect timed out");
                            }
                        }

                        retries -= 1;
                        if retries == 0 {
                            if device.is_connected().await? {
                                debug!("failed to connect after {max_retries} retries but will still try");
                                break device.services().await?;
                            } else {
                                debug!("failed to connect after {max_retries} retries");
                                continue 'evt_loop;
                            }
                        }
                        sleep(wait_interval).await;
                    };
                    info!("Connected");

                    debug!("... found {} services", services.len());
                    for service in services {
                        let uuid = service.uuid().await?;
                        debug!("Service UUID: {}", &uuid);
                        if uuid == svc_uuid {
                            info!("Found target service");

                            debug!("Checking name");
                            let mut found_name = false;
                            for characteristic in service.characteristics().await? {
                                let uuid = characteristic.uuid().await?;
                                debug!("Characteristic UUID: {}", &uuid);
                                if uuid == proxy_name_char_uuid {
                                    info!("Found name characteristic");
                                    if characteristic.flags().await?.read {
                                        debug!("Reading characteristic value");
                                        let value = characteristic.read().await?;
                                        let proxy_name = String::from_utf8_lossy(&value);
                                        if proxy_name == device_name {
                                            found_name = true;
                                            break;
                                        }
                                        debug!("Read str: {:x?}", &proxy_name);
                                    }
                                }
                            }
                            if !found_name {
                                debug!("Skipping this device");
                                continue;
                            }

                            debug!("ensuring paired and trusted");

                            if !device.is_paired().await? {
                                debug!("pairing");
                                device.pair().await?;
                            }
                            if !device.is_trusted().await? {
                                debug!("trusting");
                                device.set_trusted(true).await?;
                            }

                            debug!("Getting PSM");
                            for char in service.characteristics().await? {
                                let uuid = char.uuid().await?;
                                debug!("Characteristic UUID: {}", &uuid);
                                if uuid == psm_char_uuid {
                                    info!("Found psm characteristic");
                                    if char.flags().await?.read {
                                        debug!("Reading characteristic value");
                                        let value = char.read().await?;
                                        debug!("Read value: {:x?}", &value);
                                        let str_psm = String::from_utf8_lossy(&value);
                                        match str_psm.parse::<u16>() {
                                            Ok(psm) => {
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
    let target_sa = SocketAddr::new(device.remote_address().await?, addr_type, psm);

    debug!("Connecting to L2CAP CoC at {:?}", &target_sa);
    let stream = Stream::connect(target_sa).await?;

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
    let mtu_as_cap = stream.as_ref().recv_mtu()?;
    let mut message_buf = vec![0u8; mtu_as_cap as usize];
    stream.read(&mut message_buf).await.expect("read failed");
    Ok(format!("{}", String::from_utf8_lossy(&message_buf)))
}

async fn connect(device: &Device) -> bluer::Result<Vec<gatt::remote::Service>> {
    device.connect().and_then(|_| device.services()).await
}
