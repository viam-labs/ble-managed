//! Runs the Viam grill proxy.

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

/// Service UUID for advertised local proxy device name characteristic and remote PSM
/// characteristic.
const VIAM_SERVICE_UUID: uuid::Uuid = uuid!("79cf4eca-116a-4ded-8426-fb83e53bc1d7");

/// Characteristic UUID for our device name (us as a peripheral).
const MANAGED_MACHINE_NAME_CHAR_UUID: uuid::Uuid = uuid!("918ce61c-199f-419e-b6d5-59883a0049d7");

/// Characteristic UUID for their device name (the proxy as a central).
const SOCKS_PROXY_NAME_CHAR_UUID: uuid::Uuid = uuid!("918ce61c-199f-419e-b6d5-59883a0049d8");

/// Characteristic UUID for remote PSM.
const PSM_CHARACTERISTIC_UUID: uuid::Uuid = uuid!("ab76ead2-b6e6-4f12-a053-61cd0eed19f9");

/// Utility function to return ok from box.
async fn return_ok() -> ReqResult<()> {
    Ok(())
}

/// Utility function to return hardcoded passkey from box.
async fn return_hardcoded_passkey() -> ReqResult<u32> {
    Ok(123456)
}

/// Advertises a BLE device with the Viam service UUID and two characteristics: one from which the
/// name of this device can be read, and one to which the proxy device name can be be written. Once
/// a name is written, scans for another BLE device with that proxy device name and a corresponding
/// Viam service UUID and PSM characteristic. It then returns the device, the discoverd PSM, and
/// the agent handle.
async fn find_viam_proxy_device_and_psm() -> Result<(bluer::Device, u16, AgentHandle)> {
    debug!("Getting bluer session");
    let session = bluer::Session::new().await?;

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

    // Use `unwrap` here to cause a fatal error in the event on inability to get the managed device
    // name from `/etc/viam.json`. There is no default value for this, and the user has likely
    // removed or edited `/etc/viam.json`.
    let managed_device_name = env::get_managed_device_name().await.unwrap();

    let advertised_ble_name = env::get_advertised_ble_name().await?;
    // This alias is what shows up in pairing requests.
    adapter.set_alias(advertised_ble_name.clone()).await?;

    info!("Advertising self='{advertised_ble_name}' on service='{VIAM_SERVICE_UUID}' characteristic='{SOCKS_PROXY_NAME_CHAR_UUID}'");
    let proxy_device_name = peripheral::advertise_and_find_proxy_device_name(
        &adapter,
        managed_device_name,
        advertised_ble_name,
        VIAM_SERVICE_UUID,
        MANAGED_MACHINE_NAME_CHAR_UUID,
        SOCKS_PROXY_NAME_CHAR_UUID,
    )
    .await?;
    info!("Proxy device is '{proxy_device_name}'");

    let (device, psm) = central::find_device_and_psm(
        &adapter,
        proxy_device_name,
        VIAM_SERVICE_UUID,
        SOCKS_PROXY_NAME_CHAR_UUID,
        PSM_CHARACTERISTIC_UUID,
    )
    .await?;
    info!(
        "Found device='{}' that is waiting for l2cap connections on psm='{psm}'; connecting",
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
            find_result = find_viam_proxy_device_and_psm() => {
                match find_result {
                    Ok((device, psm, handle)) => {
                        if socks::start_proxy(device, psm).await? {
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
