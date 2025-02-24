//! Defines peripheral logic.

use anyhow::{anyhow, Result};
use bluer::{
    adv::Advertisement,
    gatt::local::{
        characteristic_control, Application, Characteristic, CharacteristicControlEvent,
        CharacteristicRead, CharacteristicWrite, CharacteristicWriteMethod, Service,
    },
    Adapter,
};
use futures::{pin_mut, FutureExt, StreamExt};
use log::{debug, info};
use std::{str::from_utf8, time::Duration};
use tokio::io::AsyncReadExt;
use uuid::Uuid;

/// Advertises a peripheral device:
///
/// - with adapter `adapter`
/// - with a service IDed as `svc_uuid`
/// - with a read characteristic IDed as `managed_machine_name_char_uuid` with `device_name`
/// - with a write characteristic IDed as `socks_proxy_name_char_uuid`
///
/// Waits for a BLE central to write a UTF8-encoded string to that characteristic and returns the
/// written value (or an error).
pub async fn advertise_and_find_proxy_device_name(
    adapter: &Adapter,
    device_name: String,
    advertised_ble_name: String,
    svc_uuid: Uuid,
    managed_name_char_uuid: Uuid,
    proxy_name_char_uuid: Uuid,
) -> Result<String> {
    let le_advertisement = Advertisement {
        advertisement_type: bluer::adv::Type::Peripheral,
        service_uuids: vec![svc_uuid].into_iter().collect(),
        discoverable: Some(true),
        min_interval: Some(Duration::from_millis(20)),
        max_interval: Some(Duration::from_millis(100)),
        local_name: Some(advertised_ble_name),
        ..Default::default()
    };
    let _adv_handle = Some(adapter.advertise(le_advertisement).await?);
    info!("Registered advertisement");

    let device_name_copy = device_name.clone();
    let (char_control, char_handle) = characteristic_control();
    let app = Application {
        services: vec![Service {
            uuid: svc_uuid,
            primary: true,
            characteristics: vec![
                Characteristic {
                    uuid: managed_name_char_uuid,
                    read: Some(CharacteristicRead {
                        read: true,
                        // this is public info
                        encrypt_read: false,
                        encrypt_authenticated_read: false,
                        secure_read: false,
                        fun: Box::new(move |_| {
                            let device_name_clone = device_name_copy.clone();
                            async move { Ok(device_name_clone.as_bytes().to_vec()) }.boxed()
                        }),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                Characteristic {
                    uuid: proxy_name_char_uuid,
                    write: Some(CharacteristicWrite {
                        write: true,
                        encrypt_write: false,
                        encrypt_authenticated_write: false,
                        secure_write: false,
                        method: CharacteristicWriteMethod::Io,
                        ..Default::default()
                    }),
                    control_handle: char_handle,
                    ..Default::default()
                },
            ],
            ..Default::default()
        }],
        ..Default::default()
    };
    let _app_handle = Some(adapter.serve_gatt_application(app).await?);

    info!("Advertising proxy device name char to be written to. Local device name: {device_name}");

    info!("In healthy and idle state. Waiting for proxy device name to be written");
    pin_mut!(char_control);

    loop {
        let evt = char_control.next().await;
        match evt {
            Some(CharacteristicControlEvent::Write(req)) => {
                debug!("Accepting write request event with MTU {}", req.mtu());
                let device_addr = req.device_address();

                let mut read_buf = vec![0; req.mtu()];
                let mut reader = req.accept()?;
                let num_bytes = reader.read(&mut read_buf).await?;
                let trimmed_read_buf = &read_buf[0..num_bytes];
                match from_utf8(trimmed_read_buf) {
                    Ok(proxy_device_name_str) => {
                        // Attempt to pair with the device that wrote its name to our characteristic.
                        let device = adapter.device(device_addr)?;
                        if !device.is_paired().await? {
                            info!(
                                "Pairing with device {} that wrote its proxy name",
                                device_addr
                            );
                            device.pair().await?;
                        }
                        if !device.is_trusted().await? {
                            // Trusting should also resolve any addresses that require resolution.
                            info!("Trusting device {} that wrote its proxy name", device_addr);
                            device.set_trusted(true).await?;
                        }

                        return Ok(proxy_device_name_str.to_string());
                    }
                    Err(e) => {
                        return Err(anyhow!(
                            "written proxy device name is not a UT8-encoded string: {e}"
                        ));
                    }
                }
            }
            Some(CharacteristicControlEvent::Notify(notifier)) => {
                debug!(
                    "Should not happen: accepting notify request event with MTU {}",
                    notifier.mtu()
                );
            }
            None => break,
        }
    }

    Err(anyhow!("failed to collect a proxy device name"))
}
