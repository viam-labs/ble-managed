# Phone Proxy

The phone proxy is a piece of the BLE-SOCKS bridge. It is meant to serve as an
example of a flutter app on Android or iPhone. See the
[flutter-ble](https://github.com/viamrobotics/flutter-ble) repository for the
underlying library. The phone proxy is automatically connected to by SOCKS
forwarding processes that will attempt to route SOCKS requests through the
phone proxy.

# Building

* [Install flutter](https://docs.flutter.dev/get-started/install) on your
  machine and ensure you can run apps on a connected device.

To run the app, call `flutter run` from within this directory with a physical
phone connected via serial. The app cannot be run with a simulated device, as
bluetooth functionality is limited to physical devices.
