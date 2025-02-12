# SOCKS forwarder

The SOCKS forwarder is a piece of the BLE-SOCKS bridge. It is meant to run as a
systemd service on a Debian linux SBC (such as a rock4C+.) See the [socks-forwarder
module](https://app.viam.com/module/viam/socks-forwarder) module for a Viam module that
can interact with the SOCKS forwarder systemd service. The SOCKS forwarder will
automatically create connections to phone proxies (see ../phone_proxy) and can route all
SOCKS requests through those proxies.

# Requirements for installation

The SOCKS forwarder requires an installed version of bluez >= 5.79. On older OSes that
come with a version < 5.79, it is easiest to install bluez from source. Use the
`etc/install_bluez.sh` script at the top of this repository to do so on Debian images.
If you encounter any installation errors, please report them through the "Issues" tab.

# Installing

Can only install on debian-based linux. Installing through the `dpkg -i` command as shown
below will also immediately start and enable the service.

`sudo dpkg -i [deb-file]`

# Custom advertised characteristic and name

The BLE characteristic that will be advertised and discoverable via a mobile device is the
`id` field of the Viam cloud config at the path `/etc/viam.json`. It is not otherwise
customizable. **It MUST be specified or the `socks-forwarder` service will fail to
start**.

The advertised BLE name (what appears as the device name in most bluetooth
discovery menus) can be specified in the first line of a file at the path
`/etc/advertised_ble_name.txt`. It defaults to "Viam SOCKS forwarder" and does not
need to be specified.

## Monitoring tips

To monitor the activity of the SOCKS forwarder, use `sudo journalctl -u socks-forwarder`
to view system logs.

## Development Tips

It can be helpful to look through various system logs to debug any issues.

- `sudo dbus-monitor --system` - monitors DBUS interactions
- `btmon` - monitors low level bluetooth interactions (HCI)
- `bluetoothctl` - CLI for controlling bluetooth adapter
	- `devices` - list known devices
	- `info <dev>` - get info on known device MAC

### Troubleshooting

* Restart bluetooth on phone and linux:
```
sudo hciconfig hci0 reset
sudo systemctl restart bluetooth
```

* Make sure `bluetoothctl` is _not_ open during proxy operation since this can
  mess with the rust based agent.

* Phone stuck in connecting even though it looks like the devices are
  connected: No known workaround other than to use `bluetoothctl` (`remove`)
  and phone to unpair from each other.

# Updating code

- Make any edits to code
- Update changelog and bump version with `dch -i`
- Update version in `Cargo.toml` to match new version in `debian/changelog`
- Run `make dpkg` to rebuild `.deb` file (ensure you are running on an `aarch64` pi-like device)
- Commit to repository or open a pull request and tag @benjirewis
- Once merged, copy `.deb` file to [socks-forwarder module](https://app.viam.com/module/viam/socks-forwarder)
- Follow directions to update the module in that repo
