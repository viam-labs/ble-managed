//! Advertises BLE device with Viam service UUID and a char where a proxy device name can be be
//! written. Once a name is written, scans for another BLE device with that name and a Viam service
//! UUID and PSM char, and opens L2CAP socket over that advertised PSM. Writes "hello" message and
//! reads response.

mod central;
mod peripheral;

use log::info;
use uuid::uuid;

/// Name to advertise as this machine.
const MANAGED_DEVICE_NAME: &str = "mac1.loc1.viam.cloud";

/// Service UUID for advertised local proxy device name characteristic and remote PSM
/// characteristic.
const VIAM_SERVICE_UUID: uuid::Uuid = uuid!("79cf4eca-116a-4ded-8426-fb83e53bc1d7");

/// Characteristic UUID for local proxy device name.
const PROXY_DEVICE_NAME_CHAR_UUID: uuid::Uuid = uuid!("918ce61c-199f-419e-b6d5-59883a0049d8");

/// Characteristic UUID for remote PSM.
const PSM_CHARACTERISTIC_UUID: uuid::Uuid = uuid!("ab76ead2-b6e6-4f12-a053-61cd0eed19f9");

#[tokio::main(flavor = "current_thread")]
async fn main() -> bluer::Result<()> {
    env_logger::init();

    let session = bluer::Session::new().await?;
    let adapter = session.default_adapter().await?;

    // Restart adapter to sever existing connections.
    //
    // TODO(low): Find a way to remove this code. I can't imagine the rock4 will be connected to
    // anything besides its last proxy device, but it's a smell to have to reconnect if a
    // connection already exists.
    adapter.set_powered(false).await?;
    adapter.set_powered(true).await?;

    let proxy_device_name = peripheral::advertise_and_find_proxy_device_name(
        &adapter,
        MANAGED_DEVICE_NAME.to_string(),
        VIAM_SERVICE_UUID,
        PROXY_DEVICE_NAME_CHAR_UUID,
    )
    .await?;

    // Restart adapter to sever connection created by mobile device.
    //
    // TODO(low): Find a way to remove this code. The mobile device should already be connected
    // at this point, can we just "invert" the GATT interaction and look for the mobile device's
    // advertised PSM?
    adapter.set_powered(false).await?;
    adapter.set_powered(true).await?;

    let (device, psm) = central::find_device_and_psm(
        &adapter,
        proxy_device_name,
        VIAM_SERVICE_UUID,
        PSM_CHARACTERISTIC_UUID,
    )
    .await?;

    let mut stream = central::connect_l2cap(&device, psm).await?;

    central::write_l2cap("hello".to_string(), &mut stream).await?;

    let msg = central::read_l2cap(&mut stream).await?;
    info!("Received message {msg}");

    device.disconnect().await?;
    Ok(())
}
