//! The Viam socks-forwarder process (runs as a systemd service.)

mod central;
mod env;
mod peripheral;
mod socks;

use anyhow::Result;
use bluer::agent::{Agent, AgentHandle, ReqResult};
use futures::FutureExt;
use log::{debug, info, warn};
use tokio::signal::unix::{signal, SignalKind};
use uuid::uuid;

/// BLE service UUID for all Viam characteristics (local and remote.)
const VIAM_SERVICE_UUID: uuid::Uuid = uuid!("79cf4eca-116a-4ded-8426-fb83e53bc1d7");

/// BLE characteristic UUID to advertise the local machine part ID on.
const MACHINE_PART_ID_CHAR_UUID: uuid::Uuid = uuid!("918ce61c-199f-419e-b6d5-59883a0049d7");

/// BLE characteristic UUID to receive mobile device names on.
const MOBILE_DEVICE_NAME_CHAR_UUID: uuid::Uuid = uuid!("918ce61c-199f-419e-b6d5-59883a0049d8");

/// BLE characteristic UUID for the remote PSM (seen by us as a central.)
const PSM_CHARACTERISTIC_UUID: uuid::Uuid = uuid!("ab76ead2-b6e6-4f12-a053-61cd0eed19f9");

/// Utility function to return ok from box.
async fn return_ok() -> ReqResult<()> {
    Ok(())
}

/// Utility function to return a hardcoded passkey from box.
async fn return_hardcoded_passkey() -> ReqResult<u32> {
    Ok(123456)
}

/// Advertises a BLE device with the Viam service UUID and two characteristics: one from which the
/// machine part id of this device can be read, and one to which a mobile device name can be be
/// written. Once a name is written, scans for another BLE device with that mobile device name and
/// a corresponding Viam service UUID and PSM characteristic. It then returns the device, the
/// discovered PSM, and the agent handle.
async fn find_viam_mobile_device_and_psm() -> Result<(bluer::Device, u16, AgentHandle)> {
    // Get the machine part id from `/etc/viam.json` and retry upon failure. A non-existent or
    // corrupted `/etc/viam.json` likely means the machine has not yet been provisioned. There
    // will be no traffic to forward until the device is provisioned.
    let mut logged_no_machine_part_id_warning = false;
    let machine_part_id = loop {
        match env::get_machine_part_id().await {
            Ok(name) => break name,
            Err(e) => {
                if !logged_no_machine_part_id_warning {
                    warn!("{e}");
                    warn!("SOCKS forwarder not functional until machine part ID can be fetched");
                    logged_no_machine_part_id_warning = true;
                }

                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
        }
    };
    info!("Machine part ID fetched from `/etc/viam.json`: {machine_part_id}");

    debug!("Getting bluer session");
    let session = bluer::Session::new().await?;

    debug!("Registering custom agent");
    let agent = Agent {
        request_default: true,
        request_pin_code: None,
        display_pin_code: None,
        request_passkey: Some(Box::new(move |_req| {
            debug!("auto generating passkey 123456");
            return_hardcoded_passkey().boxed()
        })),
        display_passkey: None,
        request_confirmation: Some(Box::new(move |req| {
            debug!("auto confirming passkey {}", req.passkey);
            return_ok().boxed()
        })),
        request_authorization: Some(Box::new(|_| {
            debug!("auto accepting pair");
            return_ok().boxed()
        })),
        authorize_service: Some(Box::new(|_| return_ok().boxed())),
        ..Default::default()
    };
    let handle = session.register_agent(agent).await?;

    debug!("Getting default adapter");
    let adapter = session.default_adapter().await?;
    if !adapter.is_powered().await? {
        adapter.set_powered(true).await?;
    }
    log_adapter_info(&adapter).await?;

    let advertised_ble_name = env::get_advertised_ble_name().await?;
    // This alias is what shows up in pairing requests.
    adapter.set_alias(advertised_ble_name.clone()).await?;

    info!("Advertising self='{advertised_ble_name}' on service='{VIAM_SERVICE_UUID}' characteristic='{MOBILE_DEVICE_NAME_CHAR_UUID}'");
    let mobile_device_name = peripheral::advertise_and_find_mobile_device_name(
        &adapter,
        machine_part_id,
        advertised_ble_name,
        VIAM_SERVICE_UUID,
        MACHINE_PART_ID_CHAR_UUID,
        MOBILE_DEVICE_NAME_CHAR_UUID,
    )
    .await?;
    info!("Mobile device name is '{mobile_device_name}'");

    let (device, psm) = central::find_device_and_psm(
        &adapter,
        mobile_device_name,
        VIAM_SERVICE_UUID,
        MOBILE_DEVICE_NAME_CHAR_UUID,
        PSM_CHARACTERISTIC_UUID,
    )
    .await?;
    info!(
        "Found device at address '{}' that is waiting for l2cap connections on psm '{psm}'; connecting",
        device.remote_address().await?
    );

    Ok((device, psm, handle))
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    env_logger::init();
    info!("Started the SOCKS forwarder");

    let mut sigterm = signal(SignalKind::terminate())?;
    let mut sigint = signal(SignalKind::interrupt())?;

    loop {
        tokio::select! {
            find_result = find_viam_mobile_device_and_psm() => {
                match find_result {
                    Ok((device, psm, handle)) => {
                        if socks::start_forwarder(device, psm).await? {
                            continue;
                        }

                        drop(handle);
                        break;
                    },
                    Err(e) => {
                        warn!("Error while scanning for mobile device: {e}; restarting the SOCKS forwarder");
                        continue;
                    }
                }
            }
            _ = sigterm.recv() => {
                info!("Received SIGTERM signal while scanning for mobile device; stopping the SOCKS forwarder");
                break;
            },
            _ = sigint.recv() => {
                info!("Received SIGINT signal while scanning for mobile device; stopping the SOCKS forwarder");
                break;
            }
        }
    }

    info!("Stopped the SOCKS forwarder");
    Ok(())
}

// Logs (at debug level) all reported properties for the adapter.
async fn log_adapter_info(adapter: &bluer::Adapter) -> Result<()> {
    let mut properties_log = String::new();

    properties_log.push_str("Bluetooth adapter properties:\n");
    properties_log.push_str("{\n");

    properties_log.push_str(&format!("\tName: {}\n", adapter.name()));
    if let Ok(addr) = adapter.address().await {
        properties_log.push_str(&format!("\tAddress: {addr}\n"));
    }
    if let Ok(addr_type) = adapter.address_type().await {
        properties_log.push_str(&format!("\tAddress type: {addr_type}\n"));
    }
    if let Ok(alias) = adapter.alias().await {
        properties_log.push_str(&format!("\tAlias: {alias}\n"));
    }
    if let Ok(class) = adapter.class().await {
        properties_log.push_str(&format!("\tClass: {class}\n"));
    }
    if let Ok(powered) = adapter.is_powered().await {
        properties_log.push_str(&format!("\tPowered: {powered}\n"));
    }
    if let Ok(discoverable) = adapter.is_discoverable().await {
        properties_log.push_str(&format!("\tDiscoverable: {discoverable}\n"));
    }
    if let Ok(pairable) = adapter.is_pairable().await {
        properties_log.push_str(&format!("\tPairable: {pairable}\n"));
    }
    if let Ok(pairable_to) = adapter.pairable_timeout().await {
        properties_log.push_str(&format!("\tPairable timeout: {pairable_to}\n"));
    }
    if let Ok(discovering) = adapter.is_discovering().await {
        properties_log.push_str(&format!("\tDiscovering: {discovering}\n"));
    }
    // UUIDs are available as a property but likely not useful to log.
    if let Ok(modalias) = adapter.modalias().await {
        properties_log.push_str(&format!("\tModalias: {modalias:?}\n"));
    }
    if let Ok(active_instances) = adapter.active_advertising_instances().await {
        properties_log.push_str(&format!(
            "\tActive advertising instances: {active_instances}\n"
        ));
    }
    if let Ok(supported_instances) = adapter.supported_advertising_instances().await {
        properties_log.push_str(&format!(
            "\tSupported advertising instances: {supported_instances}\n"
        ));
    }
    if let Ok(supported_includes) = adapter.supported_advertising_system_includes().await {
        properties_log.push_str(&format!(
            "\tSupported system includes: {supported_includes:?}\n"
        ));
    }
    if let Ok(supported_secondaries) = adapter.supported_advertising_secondary_channels().await {
        properties_log.push_str(&format!(
            "\tSupported secondary channels: {supported_secondaries:?}\n"
        ));
    }
    if let Ok(supported_capabilities) = adapter.supported_advertising_capabilities().await {
        properties_log.push_str(&format!(
            "\tSupported advertising capabilities: {supported_capabilities:?}\n"
        ));
    }
    if let Ok(supported_features) = adapter.supported_advertising_features().await {
        properties_log.push_str(&format!(
            "\tSupported advertising features: {supported_features:?}\n"
        ));
    }

    properties_log.push_str("}");

    debug!("{}", properties_log);
    Ok(())
}
