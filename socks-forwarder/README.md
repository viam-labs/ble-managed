# SOCKS forwarder

The SOCKS forwarder is a piece of the BLE-SOCKS bridge. It is meant to run as a
systemd service on a linux SBC (such as a rock4C+). See the [socks-forwarder
module](https://app.viam.com/module/viam/socks-forwarder) module for a Viam
module that can interact with the SOCKS forwarder systemd service. The SOCKS
forwarder will automatically create connections to phone proxies (see
../phone_proxy) and can route all SOCKS requests through those proxies. 

# Requirements for installation

The SOCKS forwarder requires an installed version of bluez >= 5.60. On older OSes that
come with a version < 5.60, it is easiest to install bluez from source. Use the
`etc/install_bluez.sh` script at the top of this repository to do so. If you encounter any
installation errors, please report them through the "Issues" tab.

# Installing

Can only install on debian-based linux. Installing through the `dpkg -i` command as shown
below will also immediately start and enable the service.

`make dpkg && sudo dpkg -i [deb-file]`

# Building from source

Can only install on debian-based linux.

* Build and install latest bluez (https://github.com/bluez/bluez) from source.

Run `make setup` (will only try `apt`) and `make build` to build. Run `make
run` to run.

# Custom advertised characteristic and name

The BLE characteristic that will be advertised and discoverable via a mobile
device is the `fqdn` field of the Viam cloud config at the path
`/etc/viam.json`. It is not otherwise customizable.

The advertised BLE name (what appears as the device name in most bluetooth
discovery menus) can be specified in the first line of a file at the path
`/etc/advertised_ble_name.txt`. It defaults to "Viam SOCKS forwarder".

## Monitoring tips

To monitor the activity of the SOCKS forwarder, use `sudo journalctl -u socks-forwarder`
to view system logs.

## Development Tips

- `btmon` - monitors low level bluetooth interactions
- `bluetoothctl` - CLI for controlling bluetooth adapter
	- `devices` - list known devices
	- `info <dev>` - get info on known device MAC

### Scanning for devices in `bluetoothctl`
`menu scan`
`clear`
`transport le`
`uuids <svc ids>`
`back`
`scan on`

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
