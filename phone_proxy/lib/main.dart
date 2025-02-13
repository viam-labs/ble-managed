import 'dart:async';
import 'dart:convert';
import 'dart:typed_data';

import 'package:blev/ble.dart';
import 'package:blev/ble_central.dart';
import 'package:blev/ble_peripheral.dart';
import 'package:blev/ble_socket.dart';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:logger/logger.dart';
import 'package:permission_handler/permission_handler.dart';
import 'package:socks5_proxy/socks_server.dart';

// The following three classes are examples of a basic flutter app setup.
// Replace them with the implementation of your flutter app.
class MyApp extends StatelessWidget {
  const MyApp({super.key});

  @override
  Widget build(BuildContext context) {
    return const MaterialApp(
        title: 'Phone Proxy', home: MyHomePage(title: 'Phone Proxy'));
  }
}
class MyHomePage extends StatefulWidget {
  const MyHomePage({super.key, required this.title});

  final String title;

  @override
  State<MyHomePage> createState() => _MyHomePageState();
}
class _MyHomePageState extends State<MyHomePage> {
  _MyHomePageState() {
    loadData();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
        appBar: AppBar(
          backgroundColor: Theme.of(context).colorScheme.inversePrimary,
          title: Text(widget.title),
        ));
  }

  Future<void> loadData() async {
    while (true) {
      await Future<void>.delayed(const Duration(seconds: 1));
      setState(() {});
    }
  }
}

// `main` is an example of how your flutter app might call into `startBLESocksPhoneProxy`
// Specifically, you will have to provide the values for `mobileDevice` and
// `machineToManage`.
void main() {
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

  mainLoop(mobileDevice, machineToManage, true);
}

void mainLoop(String mobileDevice, machineToManage, bool shouldCallRunApp) {
  runZonedGuarded(
    () {
      startBLESocksPhoneProxy(mobileDevice, machineToManage);
      if (shouldCallRunApp) {
        runApp(const MyApp());
      }
    }, (error, stackTrace) {
      if (error is L2CapDisconnectedError) {
        // You may want to execute custom code in this block. Something like notifying the
        // app user that there's been a disconnection and to move back in range of the
        // grill.

        logger.w('disconnection detected; restarting pairing process');
        // Restart zone but don't call runApp to avoid zone mismatch.
        mainLoop(mobileDevice, machineToManage, false);
      } else {
        logger.e('unexpected error running BLE-SOCKS phone proxy: $error');
      }
    },
  );
}

/* No need to mutate code beneath this line. */

var logger = Logger(printer: SimplePrinter(colors: false));

// Hardcoded Viam BLE UUIDs known by both this code and SOCKS forwarder code.
const viamSvcUUID = '79cf4eca-116a-4ded-8426-fb83e53bc1d7';
const viamSocksProxyPSMCharUUID = 'ab76ead2-b6e6-4f12-a053-61cd0eed19f9';
const viamManagedMachineNameCharUUID = '918ce61c-199f-419e-b6d5-59883a0049d7';
const viamSocksProxyNameCharUUID = '918ce61c-199f-419e-b6d5-59883a0049d8';

// Give some BLE operations a few retries for resiliency.
const numRetries = 3;

void startBLESocksPhoneProxy(String mobileDevice, machineToManage) {
  WidgetsFlutterBinding.ensureInitialized();
  Permission.bluetoothConnect
      .request()
      .then((status) => Permission.bluetoothScan.request())
      .then((status) => Permission.bluetoothAdvertise.request())
      .then((status) {
    BlePeripheral.create().then((blePeriph) {
      final stateStream = blePeriph.getState();
      late StreamSubscription<AdapterState> streamSub;
      streamSub = stateStream.listen((state) {
        if (state == AdapterState.poweredOn) {
          streamSub.cancel();
          initializeProxy(blePeriph, mobileDevice, machineToManage);
        }
      });
    });
    BleCentral.create().then((bleCentral) {
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
    logger.e('error requesting bluetooth permissions: $error');
  });
}

Future<void> initializeProxy(BlePeripheral blePeriph, String mobileDevice, machineToManage) async {
  final (proxyPSM, proxyChanStream) = await blePeriph.publishL2capChannel();
  await advertiseProxyPSM(blePeriph, proxyPSM, mobileDevice);
  await listenAndProxySOCKS(proxyChanStream);
}

Future<void> advertiseProxyPSM(BlePeripheral blePeriph, int psm, String mobileDevice) async {
  logger.i('advertising self ($mobileDevice) and publishing SOCKS5 proxy PSM: $psm');
  await blePeriph.addReadOnlyService(viamSvcUUID, {
    viamSocksProxyNameCharUUID: mobileDevice,
    viamSocksProxyPSMCharUUID: '$psm',
  });
  await blePeriph.startAdvertising();
}

Future<void> listenAndProxySOCKS(Stream<L2CapChannel> chanStream) async {
  var chanCount = 0;
  logger.i('in healthy and idle state; scanning for devices to proxy traffic from');

  chanStream.listen((chan) async {
    final thisCount = chanCount++;
    logger.i('BLE-SOCKS bridge established and ready to handle traffic');
    final socksServerProxy = SocksServer();
    socksServerProxy.connections.listen((connection) async {
      logger.i(
          'forwarding ${connection.address.address}:${connection.port} -> ${connection.desiredAddress.address}:${connection.desiredPort}');
      await connection.forward(allowIPv6: true);
    }).onError((error) { logger.e('error listening for connections: $error'); });

    unawaited(socksServerProxy
        .addServerSocket(L2CapChannelServerSocketUtils.multiplex(chan)));
  }).asFuture();
}

Future<void> manageMachine(BleCentral bleCentral, String mobileDevice, machineToManage) async {
  logger.i('scanning for $machineToManage now');
  late StreamSubscription<DiscoveredBlePeripheral> deviceSub;
  deviceSub = bleCentral.scanForPeripherals([viamSvcUUID]).listen(
    (periphInfo) {
      deviceSub.pause();
      logger.i('found ${periphInfo.name}; connecting');
      bleCentral.connectToPeripheral(periphInfo.id).then((periph) async {
        logger.i('connected to $machineToManage');

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
          logger.e("expected service missing; disconnecting");
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
                      char != null && char.id == viamManagedMachineNameCharUUID,
                  orElse: () => null);
          if (periphNameChar != null) {
            break;
          }
        }
        if (periphNameChar == null) {
          logger.e(
              'did not find needed periph name char after discovery; disconnecting');
          await periph.disconnect();
          deviceSub.resume();
          return;
        }

        final periphName = utf8.decode((await periphNameChar.read())!);
        if (periphName != machineToManage) {
          logger.e('found a different machine $periphName; disconnecting');
          await periph.disconnect();
          deviceSub.resume();
          return;
        }

        deviceSub.cancel();

        final proxyNameChar = viamSvc.characteristics
            .cast<BleCharacteristic?>()
            .firstWhere(
                (char) => char != null && char.id == viamSocksProxyNameCharUUID,
                orElse: () => null);
        if (proxyNameChar == null) {
          logger.w('did not find needed PSM char after discovery');
          await Future<void>.delayed(const Duration(seconds: 1));
          logger.i('disconnecting from machine and trying again');
          await periph.disconnect();
          unawaited(manageMachine(bleCentral, mobileDevice, machineToManage));
          return;
        }

        logger.i('matched desired machine $periphName; writing our name now');

        try {
          await proxyNameChar.write(Uint8List.fromList(mobileDevice.codeUnits));
        } catch (error) {
          logger.e(
              'error writing characteristic: $error; disconnecting from machine and trying again');
          await periph.disconnect();
          unawaited(manageMachine(bleCentral, mobileDevice, machineToManage));
          return;
        }

        logger.i('machine to manage knows our name and we will wait for a connection');
      }).catchError((error) {
        logger.e('error establishing connection with machine to manage: $error; will try again');
        unawaited(manageMachine(bleCentral, mobileDevice, machineToManage));
      });
    },
    onError: (Object e) => logger.e('manageMachine failed: $e'),
  );
}
