# ble-managed

Code to run a BLE-SOCKS bridge. See READMEs for socks-forwarder (rust proxy) and
phone_proxy (flutter proxy) in their respective directories.

Also see documentation for the [socks-forwarder
module](https://app.viam.com/module/viam/socks-forwarder) that interfaces with
the socks-forwarder systemd service, and the flutter-ble library leveraged by
[phone_proxy](https://github.com/viamrobotics/flutter-ble).

## Development

### Prerequisites
* SBC running a Debian-based OS with an aarch64 architecture
  * The SBC must have a bluetooth adapter that supports bluetooth version 5.0 or higher
  * A config for a Viam machine must be stored in `/etc/viam.json`. Specifically, the `id` field is read by the SOCKS forwarder as the BLE characteristic that will be advertised and discoverable from nearby devices.
  * If developing on the `socks-forwarder`, [rust](https://www.rust-lang.org) should also be installed.
* An iPhone or Android mobile device connected via USB to a separate computer
* A working mobile development setup
  * A working flutter installation
  * A working XCode installation if using an iPhone
    * flutter doctor should show no iOS issues and flutter devices should show an iPhone connected over USB 
  * A working Android Studio installation if using an Android
    * flutter doctor should show no Android issues and flutter devices should show an Android connected over USB 

### Setup
* Ensure your SBC has an internet connection
While the system will eventually work without it; the initial provisioning of the device requires an internet connection for now
* Check bluetoothctl version on the SBC
  * Run bluetoothctl version on the SBC; verify that it shows version <b>5.79</b> or greater
    * If the command hangs or does not show any version, ensure any hciattach commands have been run for custom, external bluetooth adapters
    * If the displayed version is < 5.79, run this [script](https://github.com/viam-labs/ble-managed/blob/main/etc/install_bluez.sh) to install the latest version of bluez, bluetoothctl, and the bluetooth systemd service
* Configure a Viam machine on [app.viam.com](https://app.viam.com). If developing on the `socks-forwarder`, it will make sense to run the `viam-server` separately, but otherwise you can install the `viam-agent` to handle the management of the `viam-server`.
  * Add [this fragment](https://app.viam.com/fragment/c799e8c9-3a8a-4df4-8c6d-1b9851fcd529/json) to your machine. On downloading and running the module, the `viam-server` will install and start the `socks-forwarder` systemd service.
  * <b>Optional:</b> Create an `/etc/advertised_ble_name.txt` file on the SBC with a name for the BLE device (on the first line) if desired. This is the name that will appear in pairing requests and mobile device bluetooth discovery menus. The name defaults to “Viam SOCKS forwarder” if none is specified in `/etc/advertised_ble_name.txt`.
* Ensure `socks-forwarder` is running.
  * If developing on the `socks-forwarder`, make sure the systemd service is not running.
    * `sudo systemctl stop socks-forwarder` or `sudo systemctl disable socks-forwarder`
    * Start the `socks-forwarder` by running `cargo run` in the `socks-forwarder` directory
  * Otherwise, run `sudo systemctl status socks-forwarder`
    * The service should be “enabled” and “active”; those states mean that the service is running and will start up whenever the SBC starts up, respectively
    * You should see the log `In healthy and idle state. Waiting for proxy device name to be written` as the most recent log from the service.
* Run [this sample flutter app](https://github.com/viam-labs/ble-managed/tree/main/phone_proxy) on a nearby mobile device (example of flutter logic to later be run in a user mobile app) from your separate computer
  * Ensure mobile device is connected via USB to your computer and visible through flutter devices
    * Run flutter doctor and look for iOS or Android issues if device is not visible
  * Clone the repository if you have not already
    * `git clone git@github.com:viam-labs/ble-managed.git`
  * Modify machineToManage in phone_proxy/lib/main.dart to be the part ID of the Viam machine (available under “Connect” -> “Connection Details” on [app.viam.com](app.viam.com))
    * From the customer’s app, this part ID could be found via a QR Code or some other physical marking on the pre-provisioned device
  * Enter the phone_proxy directory
    * `cd phone_proxy`
  * Run the sample flutter app
    * flutter run
  * Accept pairing request from the SBC when prompted (name of device should be the same as what’s in `/etc/advertised_ble_name.txt` on the SBC)
  * You should see a “'BLE-SOCKS bridge established and ready to handle traffic” message in flutter/xcode logs (iPhone) or flutter/gradle logs (Android)
    * The same log is output from the socks-forwarder systemd service if you would like to confirm establishment from the other side
