//! Advertises a BLE device with the Viam service UUID and a characteristic where a
//! proxy device name can be be written to. Once a name is written, it scans for another
//! BLE device with that name and a corresponding Viam service UUID and PSM characteristic.
//! It then opens an L2CAP CoC socket over that advertised PSM. Finally it writes a "hello"
//! message and reads one message.

mod central;
mod peripheral;

use bluer::agent::{Agent, ReqResult};
use futures::FutureExt;
use log::{debug, info};
use uuid::uuid;

/// Name to advertise as this machine.
const MANAGED_DEVICE_NAME: &str = "mac1.loc1.viam.cloud";

/// Service UUID for advertised local proxy device name characteristic and remote PSM
/// characteristic.
const VIAM_SERVICE_UUID: uuid::Uuid = uuid!("79cf4eca-116a-4ded-8426-fb83e53bc1d7");

/// Characteristic UUID for our device name (us as a peripheral).
const MANAGED_MACHINE_NAME_CHAR_UUID: uuid::Uuid = uuid!("918ce61c-199f-419e-b6d5-59883a0049d7");

/// Characteristic UUID for their device name (the proxy as a central).
const SOCKS_PROXY_NAME_CHAR_UUID: uuid::Uuid = uuid!("918ce61c-199f-419e-b6d5-59883a0049d8");

/// Characteristic UUID for remote PSM.
const PSM_CHARACTERISTIC_UUID: uuid::Uuid = uuid!("ab76ead2-b6e6-4f12-a053-61cd0eed19f9");

async fn return_ok() -> ReqResult<()> {
    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> bluer::Result<()> {
    env_logger::init();

    debug!("getting bluer session");
    let session = bluer::Session::new().await?;

    let agent = Agent {
        request_default: true,

        // Don't want to use these
        request_pin_code: None,
        display_pin_code: None,
        request_passkey: None,
        display_passkey: None,

        // TODO(erd->benji): These work for POC but production where some on screen device should
        // confirm.
        request_confirmation: Some(Box::new(|_| return_ok().boxed())),
        request_authorization: Some(Box::new(|_| return_ok().boxed())),
        authorize_service: Some(Box::new(|_| return_ok().boxed())),
        ..Default::default()
    };
    let handle = session.register_agent(agent).await?;

    debug!("getting default adapter");
    let adapter = session.default_adapter().await?;
    if !adapter.is_powered().await? {
        adapter.set_powered(true).await?;
    }

    debug!("advertising self='{MANAGED_DEVICE_NAME}' on service='{VIAM_SERVICE_UUID}' characteristic='{SOCKS_PROXY_NAME_CHAR_UUID}'");
    let proxy_device_name = peripheral::advertise_and_find_proxy_device_name(
        &adapter,
        MANAGED_DEVICE_NAME.to_string(),
        VIAM_SERVICE_UUID,
        MANAGED_MACHINE_NAME_CHAR_UUID,
        SOCKS_PROXY_NAME_CHAR_UUID,
    )
    .await?;
    debug!("proxy device is '{proxy_device_name}'");

    let (device, psm) = central::find_device_and_psm(
        &adapter,
        proxy_device_name,
        VIAM_SERVICE_UUID,
        SOCKS_PROXY_NAME_CHAR_UUID,
        PSM_CHARACTERISTIC_UUID,
    )
    .await?;
    debug!(
        "found device='{}' that is waiting for l2cap connections on psm='{psm}'; connecting",
        device.remote_address().await?
    );

    let mut stream = central::connect_l2cap(&device, psm).await?;

    central::write_l2cap("hello".to_string(), &mut stream).await?;

    let msg = central::read_l2cap(&mut stream).await?;
    info!("Received message '{msg}'");

    debug!("done. goodbye");
    drop(handle);
    Ok(())
}