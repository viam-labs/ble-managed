//! Defines peripheral logic.

use bluer::{
    adv::Advertisement,
    gatt::local::{
        Application, Characteristic, CharacteristicWrite, CharacteristicWriteMethod, ReqError,
        Service,
    },
    Adapter,
};
use futures::FutureExt;
use log::{error, info};
use std::{collections::BTreeMap, str::from_utf8, sync::mpsc::channel};
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
        local_name: Some(device_name),
        ..Default::default()
    };
    let _adv_handle = Some(adapter.advertise(le_advertisement).await?);
    info!("Registered advertisement");

    let (tx, rx) = channel();
    let app = Application {
        services: vec![Service {
            uuid: svc_uuid,
            primary: true,
            characteristics: vec![Characteristic {
                uuid: proxy_device_name_char_uuid,
                write: Some(CharacteristicWrite {
                    write: true,
                    encrypt_write: true,
                    encrypt_authenticated_write: true,
                    secure_write: true,
                    method: CharacteristicWriteMethod::Fun(Box::new(move |new_value, req| {
                        let tx = tx.clone();
                        async move {
                            // Log possible errors in this block, as only an enum of ReqError can
                            // be returned from here.
                            info!(
                                "Char write request {:?} with value {:x?}",
                                &req, &new_value
                            );
                            match from_utf8(&new_value) {
                                Ok(proxy_device_name_str) => {
                                    if let Err(e) = tx.send(proxy_device_name_str.to_string()) {
                                        error!("Failed to send proxy device name on channel: {e}");
                                        return Err(ReqError::Failed);
                                    }
                                }
                                Err(e) => {
                                    error!("Written proxy device name is not a UTF8-encoded string: {e}");
                                    return Err(ReqError::Failed);
                                }
                            }
                            Ok(())
                        }
                        .boxed()
                    })),
                    ..Default::default()
                }),
                ..Default::default()
            }],
            ..Default::default()
        }],
        ..Default::default()
    };
    let _app_handle = Some(adapter.serve_gatt_application(app).await?);

    info!(
        "Advertising proxy device name char to be written to. Local device name: {}",
        adapter.name()
    );

    rx.recv().map_err(|e| bluer::Error {
        kind: bluer::ErrorKind::Failed,
        message: format!("{e}"),
    })
}
