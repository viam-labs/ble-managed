[!WARNING]
The grill proxy code is a work in progress.

# Grill proxy

The grill proxy is a SOCKS proxy process. It will automatically create connections to phone proxies
and can route all SOCKS requests through those proxies.

# Building

Can only build on linux. Run `make setup` (will only try `apt`) and `make build` to build. Run
`make` or `make run` to run.

## Development Tips

- `btmon` - monitors low level bluetooth interactions
- `bluetoothctl` - CLI for controlling bluetooth adapter
	- `devices` - list known devices
	- `info <dev>` - get info on known device MAC

### GATT Caching

This needs to be off in order to reliably reconnect to a paired device. See https://github.com/bluez/bluer/blob/0115d074aa02dd7010415a4f972828a887d9b3ac/bluer/README.md?plain=1#L109-L112 for the `cache = no` option.

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

* Build and install latest bluez (https://github.com/bluez/bluez) from source.
