//! Runs the Viam grill proxy.

mod central;
mod env;
mod peripheral;
mod socks;

use anyhow::Result;
use bluer::agent::{Agent, AgentHandle, ReqResult};
use futures::FutureExt;
use log::{debug, info};
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

/// Utility function to return ok string from box.
async fn return_ok_string() -> ReqResult<String> {
    Ok("hello".to_string())
}

/// Utility function to return ok string from box.
async fn return_ok_u32() -> ReqResult<u32> {
    Ok(u32(12))
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

        // Add debug messages to these to see why auto confirmation/authorization does not work.
        request_pin_code: Some(Box::new(move |req| {
            debug!("requesting pin code {req:#?}");
            return_ok_string().boxed()
        })),
        display_pin_code: Some(Box::new(move |req| {
            debug!("displaying pin code {req:#?}");
            return_ok().boxed()
        })),
        request_passkey: Some(Box::new(move |req| {
            debug!("requesting passkey {req:#?}");
            return_ok_u32().boxed()
        })),
        display_passkey: Some(Box::new(move |req| {
            debug!("displaying passkey {req:#?}");
            return_ok().boxed()
        })),

        // TODO(seergrills): These work for POC but production where some on screen device should
        // confirm.
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

    let managed_device_name = env::get_managed_device_name().await?;
    let advertised_ble_name = env::get_advertised_ble_name().await?;
    // This alias is what shows up in pairing requests.
    adapter.set_alias(advertised_ble_name.clone()).await?;

    debug!("Advertising self='{advertised_ble_name}' on service='{VIAM_SERVICE_UUID}' characteristic='{SOCKS_PROXY_NAME_CHAR_UUID}'");
    let proxy_device_name = peripheral::advertise_and_find_proxy_device_name(
        &adapter,
        managed_device_name,
        advertised_ble_name,
        VIAM_SERVICE_UUID,
        MANAGED_MACHINE_NAME_CHAR_UUID,
        SOCKS_PROXY_NAME_CHAR_UUID,
    )
    .await?;
    debug!("Proxy device is '{proxy_device_name}'");

    let (device, psm) = central::find_device_and_psm(
        &adapter,
        proxy_device_name,
        VIAM_SERVICE_UUID,
        SOCKS_PROXY_NAME_CHAR_UUID,
        PSM_CHARACTERISTIC_UUID,
    )
    .await?;
    debug!(
        "Found device='{}' that is waiting for l2cap connections on psm='{psm}'; connecting",
        device.remote_address().await?
    );

    Ok((device, psm, handle))
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    env_logger::init();
    info!("Started main method");

    loop {
        let (device, psm, handle) = find_viam_proxy_device_and_psm().await?;

        if !socks::start_proxy(device, psm).await? {
            drop(handle);
            break;
        }
        drop(handle);
    }

    info!("Finished main method");
    Ok(())
}
