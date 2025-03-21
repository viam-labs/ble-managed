import 'dart:async';
import 'dart:convert';

import 'package:blev/ble.dart';
import 'package:blev/ble_central.dart';
import 'package:blev/ble_peripheral.dart';
import 'package:blev/ble_socket.dart';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:logger/logger.dart';
import 'package:permission_handler/permission_handler.dart';
import 'package:socks5_proxy/socks_server.dart';

// `mobileDevice` should be stored somewhere in a mobile app. It should be
// unique to a mobile device such that `startBLESocksPhoneProxy` can:
// - Advertise `mobileDevice` as a readable characteristic
// - Write `mobileDevice` to a characteristic on the machine to manage
// - Allow L2CAP connections from that managed machine
//
// You may want to hardcode a unique `mobileDevice` value in each instance of
// your app.
var mobileDevice = 'd3e535a.viam.cloud';

// `machineToManage` should be the machine name (FQDN) of the machine for
// which the mobile device is trying to proxy traffic. Assuming it is
// running the `socks-forwarder`, the managed machine should already be:
// - Advertising `machineToManage` as a readable characteristic
// - Adveristing a writable characteristic (encrypted) to which the mobile
//   device will need to write its `mobileDevice` value
// - Ready to establish an L2CAP connection for SOCKS forwarding once a value
//   is written to the above characteristic
//
// You may want to ask users to enter the `machineToManage` value as part
// of app setup.
//
// Future modifications to the `machineToManage` value will require a
// reinvocation of the `mainLoop` method below. You may want to use
// a `Completer` flutter object to cancel or restart the current `mainLoop`
// if this behavior is desired.
var machineToManage = 'TODO';

// The following three classes are examples of a basic flutter app setup.
// Replace them with the implementation of your flutter app.
class MyApp extends StatelessWidget {
  const MyApp({super.key});

  @override
  Widget build(BuildContext context) {
    return const MaterialApp(
      title: 'Phone Proxy',
      home: MyHomePage(title: 'Phone Proxy'),
    );
  }
}

/// Logs output to the screen of the app
class ScreenOutput extends ConsoleOutput {
  void Function(OutputEvent) callback;

  ScreenOutput(this.callback);

  @override
  void output(OutputEvent event) {
    super.output(event);
    callback(event);
  }
}

class MyHomePage extends StatefulWidget {
  const MyHomePage({super.key, required this.title});

  final String title;

  @override
  State<MyHomePage> createState() => _MyHomePageState();
}

class _MyHomePageState extends State<MyHomePage> {
  List<String> display = [];
  bool _connecting = false;
  BleCentral? _bleCentral;
  BlePeripheral? _blePeriph;

  late Logger _logger;

  _MyHomePageState() {
    _logger = Logger(
        printer: SimplePrinter(colors: false),
        output: ScreenOutput(logCallback));
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        backgroundColor: Theme.of(context).colorScheme.inversePrimary,
        title: Text(widget.title),
      ),
      body: Padding(
          padding: const EdgeInsets.all(16.0),
          child: Column(mainAxisAlignment: MainAxisAlignment.center, children: [
            const Center(
              child: Text("Press a button"),
            ),
            ElevatedButton(
              onPressed: disableConnect() ? null : onConnectPress(false),
              child: const Text('Connect'),
            ),
            ElevatedButton(
              onPressed: disableConnect() ? null : onConnectPress(true),
              child: const Text('Speed Test'),
            ),
            FilledButton(
              onPressed: () {
                _bleCentral?.reset();
                _blePeriph?.reset();
                setState(() {
                  _bleCentral = null;
                  _blePeriph = null;
                  display = [];
                });
              },
              child: const Text('Disconnect'),
            ),
            const Text("Output:"),
            Center(
              child: Text(display.join('\n')),
            ),
          ])),
    );
  }

  void logCallback(OutputEvent event) {
    if (display.length + event.lines.length > 20) {
      display = display.sublist(event.lines.length);
    }
    display.addAll(event.lines);
    setState(() {
      display = display;
    });
  }

  bool disableConnect() {
    return _connecting || _bleCentral != null || _blePeriph != null;
  }

  void startBLESocksPhoneProxy(
      String mobileDevice, machineToManage, bool speedTestMode) {
    WidgetsFlutterBinding.ensureInitialized();
    Permission.bluetoothConnect
        .request()
        .then((status) => Permission.bluetoothScan.request())
        .then((status) => Permission.bluetoothAdvertise.request())
        .then((status) {
      BlePeripheral.create().then((blePeriph) {
        setState(() {
          _blePeriph = blePeriph;
        });
        final stateStream = blePeriph.getState();
        late StreamSubscription<AdapterState> streamSub;
        streamSub = stateStream.listen((state) {
          if (state == AdapterState.poweredOn) {
            streamSub.cancel();
            initializeProxy(
                blePeriph, mobileDevice, machineToManage, speedTestMode);
          }
        });
      });
      BleCentral.create().then((bleCentral) {
        setState(() {
          _bleCentral = bleCentral;
        });
        final stateStream = bleCentral.getState();
        late StreamSubscription<AdapterState> streamSub;
        streamSub = stateStream.listen((state) {
          if (state == AdapterState.poweredOn) {
            streamSub.cancel();
            manageMachine(bleCentral, mobileDevice, machineToManage);
          }
        });
      });
    }).catchError((error) {
      _logger.e('error requesting bluetooth permissions: $error');
    });
  }

  VoidCallback onConnectPress(bool speedTestMode) {
    var machineName = speedTestMode ? 'viam-speed-test' : machineToManage;
    return () {
      setState(() {
        _connecting = true;
        display = [];
      });
      try {
        startBLESocksPhoneProxy(mobileDevice, machineName, speedTestMode);
      } on L2CapDisconnectedError {
        // You may want to execute custom code in this block. Something like notifying the
        // app user that there's been a disconnection and to move back in range of the
        // Bluetooth device.
        _logger.w('disconnection detected');
      } catch (e) {
        _logger.e('unexpected error running BLE-SOCKS phone proxy: $e');
      } finally {
        setState(() {
          _connecting = false;
        });
      }
    };
  }

  /* No need to mutate code beneath this line. */

  // Hardcoded Viam BLE UUIDs known by both this code and SOCKS forwarder code.
  final viamSvcUUID = '79cf4eca-116a-4ded-8426-fb83e53bc1d7';
  final viamSocksProxyPSMCharUUID = 'ab76ead2-b6e6-4f12-a053-61cd0eed19f9';
  final viamManagedMachineNameCharUUID = '918ce61c-199f-419e-b6d5-59883a0049d7';
  final viamSocksProxyNameCharUUID = '918ce61c-199f-419e-b6d5-59883a0049d8';

  // Give some BLE operations a few retries for resiliency.
  final numRetries = 3;

  Future<void> initializeProxy(BlePeripheral blePeriph, String mobileDevice,
      machineToManage, bool speedTestMode) async {
    final (proxyPSM, proxyChanStream) = await blePeriph.publishL2capChannel();
    await advertiseProxyPSM(blePeriph, proxyPSM, mobileDevice);
    await listenAndProxySOCKS(proxyChanStream, speedTestMode);
  }

  Future<void> advertiseProxyPSM(
      BlePeripheral blePeriph, int psm, String mobileDevice) async {
    _logger.i(
        'advertising self ($mobileDevice) and publishing SOCKS5 proxy PSM: $psm');
    await blePeriph.addReadOnlyService(viamSvcUUID, {
      viamSocksProxyNameCharUUID: mobileDevice,
      viamSocksProxyPSMCharUUID: '$psm',
    });
    await blePeriph.startAdvertising();
  }

  Future<void> listenAndProxySOCKS(
      Stream<L2CapChannel> chanStream, bool speedTestMode) async {
    _logger.i(
        'in healthy and idle state; scanning for devices to proxy traffic from');

    chanStream.listen((chan) async {
      _logger.i('BLE-SOCKS bridge established and ready to handle traffic');
      // default behavior is to not be in speedTestMode, so evaluate that first
      if (!speedTestMode) {
        final socksServerProxy = SocksServer();
        socksServerProxy.connections.listen((connection) async {
          _logger.i(
              'forwarding ${connection.address.address}:${connection.port} -> ${connection.desiredAddress.address}:${connection.desiredPort}');
          await connection.forward(allowIPv6: true);
        }).onError((error) {
          _logger.e('error listening for connections: $error');
        });
        unawaited(socksServerProxy
            .addServerSocket(L2CapChannelServerSocketUtils.multiplex(chan)));
      } else {
        SpeedTestSocket(chan, _logger);
        // measure traffic and then drop

        // send traffic back over
      }
    }).asFuture();
  }

  Future<void> manageMachine(
      BleCentral bleCentral, String mobileDevice, machineToManage) async {
    _logger.i('scanning for $machineToManage now');
    late StreamSubscription<DiscoveredBlePeripheral> deviceSub;
    deviceSub = bleCentral.scanForPeripherals([viamSvcUUID]).listen(
      (periphInfo) {
        deviceSub.pause();
        _logger.i('found ${periphInfo.name}; connecting');
        bleCentral.connectToPeripheral(periphInfo.id).then((periph) async {
          _logger.i('connected to $machineToManage');

          BleService? viamSvc;
          for (int i = 0; i < numRetries; i++) {
            viamSvc = periph.services.cast<BleService?>().firstWhere(
                (svc) => svc != null && svc.id == viamSvcUUID,
                orElse: () => null);
            if (viamSvc != null) {
              break;
            }
          }
          if (viamSvc == null) {
            _logger.e("expected service missing; disconnecting");
            await periph.disconnect();
            deviceSub.resume();
            return;
          }

          BleCharacteristic? periphNameChar;
          for (int i = 0; i < numRetries; i++) {
            periphNameChar = viamSvc.characteristics
                .cast<BleCharacteristic?>()
                .firstWhere(
                    (char) =>
                        char != null &&
                        char.id == viamManagedMachineNameCharUUID,
                    orElse: () => null);
            if (periphNameChar != null) {
              break;
            }
          }
          if (periphNameChar == null) {
            _logger.e(
                'did not find needed periph name char after discovery; disconnecting');
            await periph.disconnect();
            deviceSub.resume();
            return;
          }

          final periphName = utf8.decode((await periphNameChar.read())!);
          if (periphName != machineToManage) {
            _logger.e('found a different machine $periphName; disconnecting');
            await periph.disconnect();
            deviceSub.resume();
            return;
          }

          deviceSub.cancel();

          final proxyNameChar = viamSvc.characteristics
              .cast<BleCharacteristic?>()
              .firstWhere(
                  (char) =>
                      char != null && char.id == viamSocksProxyNameCharUUID,
                  orElse: () => null);
          if (proxyNameChar == null) {
            _logger.w('did not find needed PSM char after discovery');
            await Future<void>.delayed(const Duration(seconds: 1));
            _logger.i('disconnecting from machine and trying again');
            await periph.disconnect();
            unawaited(manageMachine(bleCentral, mobileDevice, machineToManage));
            return;
          }

          _logger
              .i('matched desired machine $periphName; writing our name now');

          try {
            await proxyNameChar
                .write(Uint8List.fromList(mobileDevice.codeUnits));
          } catch (error) {
            _logger.e(
                'error writing characteristic: $error; disconnecting from machine and trying again');
            await periph.disconnect();
            unawaited(manageMachine(bleCentral, mobileDevice, machineToManage));
            return;
          }

          _logger.i(
              'machine to manage knows our name and we will wait for a connection');
        }).catchError((error) {
          _logger.e(
              'error establishing connection with machine to manage: $error; will try again');
          unawaited(manageMachine(bleCentral, mobileDevice, machineToManage));
        });
      },
      onError: (Object e) => _logger.e('manageMachine failed: $e'),
    );
  }
}

void main() {
  runApp(const MyApp());
}

class SpeedTestSocket {
  final L2CapChannel _channel;
  final Logger _logger;

  SpeedTestSocket(this._channel, this._logger) {
    _speedTest();
  }
  Future<void> _speedTest() async {
    await _uploadTest();
    // await _write();
  }

  // The structure here should match the corresponding upload_test in the socks
  // forwarder speed test.
  Future<void> _uploadTest() async {
    try {
      const bytesPerTest = 200000;
      const numTests = 5;

      double totalReceived = 0;
      double totalElapsed = 0;
      _logger.i("starting upload speed test!");
      for (var testNum = 1; testNum <= numTests; testNum++) {
        var read = 0;
        Stopwatch stopwatch = Stopwatch()..start();
        while (read < bytesPerTest) {
          // using 5000 because that seems to average the highest speed.
          // feel free to increase or decrease this.
          final data = await _channel.read(5000);
          if (data == null) {
            return;
          }
          read += data.length;
        }
        stopwatch.stop();
        var mbReceived = read / 1000000;
        var elapsedTime = stopwatch.elapsedMilliseconds / 1000;
        var mBytesPS = mbReceived / elapsedTime;
        var mBitsPS = 8 * mBytesPS;
        _logger.i('Test #$testNum of $numTests');
        _logger
            .i('\tData received: ${mbReceived.toStringAsFixed(3)} megabytes');
        _logger.i('\tTime elapsed: ${elapsedTime.toStringAsFixed(3)} sec');
        _logger.i(
            '\tUpload Speed: ${mBytesPS.toStringAsFixed(3)} megabtyes/s (${mBitsPS.toStringAsFixed(3)} megabits/s)');

        totalReceived += mbReceived;
        totalElapsed += elapsedTime;
      }
      var avgMBytesPS = totalReceived / totalElapsed;
      var avgMBitsPS = 8 * avgMBytesPS;
      _logger.i('Upload Speed Test Summary');
      _logger
          .i('\tTotal received: ${totalReceived.toStringAsFixed(3)} megabytes');
      _logger.i('\tTotal elapsed: ${totalElapsed.toStringAsFixed(3)} sec');
      _logger.i(
          '\tAvg Upload Speed: ${avgMBytesPS.toStringAsFixed(3)} megabtyes/s (${avgMBitsPS.toStringAsFixed(3)} megabits/s)');
    } catch (err) {
      debugPrint('error reading from l2cap channel: $err');
    }
  }

  Future<void> _write() async {
    // try {
    //   await for (final data in ioSinkController.stream) {
    //     if (_completer.isCompleted) {
    //       return;
    //     }
    //     await _channel.write(Uint8List.fromList(data));
    //   }
    // } catch (err) {
    //   debugPrint('error writing to l2cap channel: $err');
    // }
  }
}
