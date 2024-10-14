# Grill proxy

The grill proxy is a SOCKS proxy process. It will automatically create connections to phone proxies
and can route all SOCKS requests through those proxies.

# Building

* Build and install latest bluez (https://github.com/bluez/bluez) from source.

Can only build on linux. Run `make setup` (will only try `apt`) and `make build` to build. Run
`make run` to run.

## Development Tips

- `btmon` - monitors low level bluetooth interactions
- `bluetoothctl` - CLI for controlling bluetooth adapter
	- `devices` - list known devices
	- `info <dev>` - get info on known device MAC

### GATT Caching

This needs to be off in order to reliably reconnect to a paired device. Set options and then restart service in `/etc/bluetooth/main.conf`:
```
ControllerMode = le
Cache = no
FastConnectable = true
```

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

* Make sure `bluetoothctl` is not open during proxy operation since this can mess with the rust based agent.

* Phone stuck in connecting even though it looks like the devices are connected:
No known workaround other than to use `bluetoothctl` (`remove`) and phone to unpair from each other. Could write a script that disconnects from all devices on the linux side periodically.
