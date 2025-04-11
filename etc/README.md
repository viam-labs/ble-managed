# etc

The `etc` folder contains various utilities to help with development and installation of
the SOCKS proxy bridge.

- `bluez_5.79-2~bpo12+1_arm64.deb`
    - The recommended way to upgrade bluez to 5.79 on a Debian 12 arm64 device (raspberry pi, radxa rock). Install with `sudo dpkg -i bluez_5.79-2~bpo12+1_arm64.deb`.
    - This deb file was generated using instructions from https://wiki.debian.org/SimpleBackportCreation and by changing the line in `debian/control` containing `systemd-dev` with `systemd-dev | systemd (<< 253-2)`.
- `install_bluez.sh`
    - Installs the latest bluez from source on radxa rock Debian 11 images.
- `commands.sh`
    - Enables kernel debug logs for l2cap_core and l2cap_sock.
- `bandwidth-measure`
    - Golang program to measure bandwidth across a running SOCKS proxy. See
      `bandwidth-measure/README.md`.
