# etc

The `etc` folder contains various utilities to help with development and installation of
the SOCKS proxy bridge.

- `install_bluez.sh`
    - Installs the latest bluez from source on radxa rock Debian 11 images.
- `commands.sh`
    - Enables kernel debug logs for l2cap_core and l2cap_sock.
- `bandwidth-measure`
    - Golang program to measure bandwidth across a running SOCKS proxy. See
      `bandwidth-measure/README.md`.
