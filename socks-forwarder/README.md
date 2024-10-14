# SOCKS forwarder

The SOCKS forwarder is a piece of the BLE-SOCKS bridge. It is meant to run as a
systemd service on a linux SBC (such as a rock4C+). See the [socks-forwarder
module](https://app.viam.com/module/viam/socks-forwarder) module for a Viam
module that can interact with the SOCKS forwarder systemd service. The SOCKS
forwarder will automatically create connections to phone proxies (see
../phone_proxy) and can route all SOCKS requests through those proxies. 

# Installing

Can only install on debian-based linux.

`make dpkg && sudo dpkg -i [deb-file]`

# Building from source

Can only install on debian-based linux.

* Build and install latest bluez (https://github.com/bluez/bluez) from source.

Run `make setup` (will only try `apt`) and `make build` to build. Run `make
run` to run.

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
