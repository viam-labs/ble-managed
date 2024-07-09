//! Defines peripheral logic.

use bluer::{
    adv::Advertisement,
    gatt::local::{
        characteristic_control, Application, Characteristic, CharacteristicControlEvent,
        CharacteristicWrite, CharacteristicWriteMethod, Service,
    },
    Adapter,
};
use futures::{pin_mut, StreamExt};
use log::{debug, info};
use std::{collections::BTreeMap, str::from_utf8};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use uuid::Uuid;

/// Manufacturer ID for LE advertisement (testing ID used for now).
const TESTING_MANUFACTURER_ID: u16 = 0xffff;

/// Advertises a peripheral device:
///
/// - with adapter `adapter`
/// - named `device_name`
/// - with a service IDed as `svc_uuid`
/// - with a characteristic IDed as `proxy_device_name_char_uuid`
///
/// Waits for a BLE central to write a UTF8-encoded string to that characteristic and returns the
/// written value (or an error).
pub async fn advertise_and_find_proxy_device_name(
    adapter: &Adapter,
    device_name: String,
    svc_uuid: Uuid,
    proxy_device_name_char_uuid: Uuid,
) -> bluer::Result<String> {
    info!("Starting advertise method");
    let mut manufacturer_data = BTreeMap::new();
    manufacturer_data.insert(
        TESTING_MANUFACTURER_ID,
        /*arbitrary data */ vec![0x21, 0x22, 0x23, 0x24],
    );
    let le_advertisement = Advertisement {
        service_uuids: vec![svc_uuid].into_iter().collect(),
        manufacturer_data,
        discoverable: Some(true),
        local_name: Some(device_name.clone()),
        ..Default::default()
    };
    let _adv_handle = Some(adapter.advertise(le_advertisement).await?);
    info!("Registered advertisement");

    let (char_control, _char_handle) = characteristic_control();
    let app = Application {
        services: vec![Service {
            uuid: svc_uuid,
            primary: true,
            characteristics: vec![Characteristic {
                uuid: proxy_device_name_char_uuid,
                write: Some(CharacteristicWrite {
                    write_without_response: true,
                    method: CharacteristicWriteMethod::Io,
                    ..Default::default()
                }),
                ..Default::default()
            }],
            ..Default::default()
        }],
        ..Default::default()
    };
    let _app_handle = Some(adapter.serve_gatt_application(app).await?);

    info!("Advertising proxy device name char to be written to. Local device name: {device_name}");

    info!("Waiting for proxy device name to be written. Press enter to quit.");
    let stdin = BufReader::new(tokio::io::stdin());
    let mut lines = stdin.lines();

    pin_mut!(char_control);

    loop {
        tokio::select! {
            _ = lines.next_line() => break,
            evt = char_control.next() => {
                match evt {
                    Some(CharacteristicControlEvent::Write(req)) => {
                        debug!("Accepting write request event with MTU {}", req.mtu());
                        let mut read_buf = vec![0; req.mtu()];
                        let mut reader = req.accept()?;
                        reader.read(&mut read_buf).await?;
                        match from_utf8(&read_buf) {
                                Ok(proxy_device_name_str) => {
                                    return Ok(proxy_device_name_str.to_string());
                                }
                                Err(e) => {
                                    return Err(bluer::Error {
                                        kind: bluer::ErrorKind::Failed,
                                        message: format!("Written proxy device name is not a UTF8-encoded string: {e}"),
                                    });
                                }
                            }
                    },
                    Some(CharacteristicControlEvent::Notify(notifier)) => {
                        debug!("Accepting notify request event with MTU {}", notifier.mtu());
                    },
                    None => break,
                }
            },
        }
    }

    Err(bluer::Error {
        kind: bluer::ErrorKind::Failed,
        message: "Failed to collect a proxy device name".to_string(),
    })
}
