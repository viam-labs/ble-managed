//! Defines central logic.

use std::{collections::HashSet, time::Duration};

use anyhow::{anyhow, Result};
use bluer::{
    AdapterEvent, Device, DeviceEvent, DeviceProperty, DiscoveryFilter, DiscoveryTransport,
};
use futures::{pin_mut, select, FutureExt, StreamExt};
use log::{debug, info};
use tokio::time::sleep;

/// Finds previously paired device and its exposed PSM:
///
/// - with adapter `adapter`
/// - named `device_name`
/// - with a service IDed as `svc_uuuid`
/// - with a characteristic IDed as `mobile_device_name_char_uuid`
/// - with a characteristic IDed as `psm_char_uuid`
///
/// Returns a handle to that device and the PSM it's advertising.
pub async fn find_device_and_psm(
    adapter: &bluer::Adapter,
    device_name: String,
    svc_uuid: uuid::Uuid,
    mobile_device_name_char_uuid: uuid::Uuid,
    psm_char_uuid: uuid::Uuid,
) -> Result<(Device, u16)> {
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
        return Err(anyhow!("must stop discovering outside of this process"));
    }

    let discover = adapter.discover_devices_with_changes().await?;
    pin_mut!(discover);

    'evt_loop: while let Some(evt) = discover.next().await {
        match evt {
            AdapterEvent::DeviceAdded(addr) => {
                let device = adapter.device(addr)?;
                let remote_addr = device.remote_address().await?;

                match device.rssi().await? {
                    Some(rssi) if rssi <= -200 => {
                        debug!("Device {remote_addr} out of range; skipping");
                        continue;
                    }
                    None if !device.is_connected().await? => {
                        debug!("Device {remote_addr} has no RSSI and not connected; skipping");
                        continue;
                    }
                    _ => {}
                }

                // It's possible the connection was lost, so try to reconnect if so.
                if !device.is_connected().await? {
                    info!("Device {remote_addr} not connected to; reconnecting now");
                    device.connect().await?;
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

                    let wait_interval = Duration::from_secs(30);

                    let changes = device.events().await?.fuse();
                    pin_mut!(changes);

                    if !device.is_services_resolved().await? {
                        if !device.is_connected().await? {
                            debug!("Reconnecting before waiting for GATT service resolution");
                            device.connect().await?;
                        }

                        debug!("Waiting for GATT services to resolve");
                        let timeout = sleep(wait_interval).fuse();
                        pin_mut!(timeout);

                        loop {
                            select! {
                                change_opt = changes.next() => {
                                    match change_opt {
                                        Some(DeviceEvent::PropertyChanged (DeviceProperty::ServicesResolved(true)) ) => {
                                            debug!("GATT services resolved");
                                            break
                                        },
                                        Some(DeviceEvent::PropertyChanged (DeviceProperty::Connected(false)) ) => {
                                            debug!("Lost connection while waiting for GATT service resolution; reconnecting");
                                            device.connect().await?;
                                        },
                                        Some(_) => { // check anyway
                                            if device.is_services_resolved().await? {
                                                debug!("GATT services resolved");
                                                break;
                                            }
                                        },
                                        None => {
                                            debug!("Changes for device stopped streaming; will restart waiting for GATT service resolution");
                                            continue 'evt_loop;
                                        },
                                    }
                                },
                                () = &mut timeout => {
                                    debug!("GATT services failed to resolve after {wait_interval:?} will restart waiting for GATT service resolution");
                                    continue 'evt_loop;
                                },
                            }
                        }
                    }
                    debug!("Getting resolved services");
                    let services = device.services().await?;

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
                                if uuid == mobile_device_name_char_uuid {
                                    info!("Found name characteristic");
                                    if characteristic.flags().await?.read {
                                        debug!("Reading characteristic value");
                                        let value = characteristic.read().await?;
                                        let found_device_name = String::from_utf8_lossy(&value);
                                        if found_device_name == device_name {
                                            found_name = true;
                                            break;
                                        }
                                        debug!("Read str: {:x?}", &found_device_name);
                                    }
                                }
                            }
                            if !found_name {
                                debug!("Skipping this device; as name characteristic did not match {device_name}");
                                continue;
                            }

                            info!("Getting PSM from characteristics");
                            for char in service.characteristics().await? {
                                let uuid = char.uuid().await?;
                                debug!("Characteristic UUID: {}", &uuid);
                                if uuid == psm_char_uuid {
                                    info!("Found PSM characteristic");
                                    if char.flags().await?.read {
                                        debug!("Reading PSM characteristic value");
                                        let value = char.read().await?;
                                        debug!("Read value: {:x?}", &value);
                                        let str_psm = String::from_utf8_lossy(&value);
                                        match str_psm.parse::<u16>() {
                                            Ok(psm) => {
                                                return Ok((device, psm));
                                            }
                                            Err(e) => {
                                                return Err(anyhow!(
                                                    "found PSM is not a valid u16: {e}"
                                                ));
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
    Err(anyhow!(
        "Desired service and characteristic combination not found"
    ))
}
