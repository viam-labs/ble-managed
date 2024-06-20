// ignore_for_file: avoid_print, public_member_api_docs
import 'dart:async';
import 'dart:convert';
import 'dart:typed_data';

import 'package:ble/ble.dart';
import 'package:ble/ble_central.dart';
import 'package:ble/ble_peripheral.dart';
import 'package:ble/ble_socket.dart';
import 'package:flutter/material.dart';
import 'package:permission_handler/permission_handler.dart';
import 'package:socks5_proxy/socks_server.dart';

List<String> lines = [];

// This should be stored somewhere in a mobile app.
var deviceName = 'd3e535ca.viam.cloud';

var machineToManage = 'mac1.loc1.viam.cloud';

const viamSvcUUID = '79cf4eca-116a-4ded-8426-fb83e53bc1d7';
const viamSocksProxyPSMCharUUID = 'ab76ead2-b6e6-4f12-a053-61cd0eed19f9';
const viamManagedMachinePSMCharUUID = '918ce61c-199f-419e-b6d5-59883a0049d8';

void main() {
  runZoned(
    () {
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
              initializeProxy(blePeriph);
            }
          });
        });
        BleCentral.create().then((bleCentral) {
          final stateStream = bleCentral.getState();
          late StreamSubscription<AdapterState> streamSub;
          streamSub = stateStream.listen((state) {
            if (state == AdapterState.poweredOn) {
              streamSub.cancel();
              manageMachine(bleCentral, machineToManage);
            }
          });
        });
      }).catchError((error) {
        print('error requesting bluetooth permissions $error');
      });

      runApp(const MyApp());
    },
    zoneSpecification: ZoneSpecification(
      print: (self, parent, zone, line) async {
        if (lines.length > 30) {
          lines.removeAt(0);
        }
        lines.add('${DateTime.now()}: $line');
        parent.print(zone, line);
      },
    ),
  );
}

Future<void> initializeProxy(BlePeripheral blePeriph) async {
  final (proxyPSM, proxyChanStream) = await blePeriph.publishL2capChannel();
  await advertiseProxyPSM(blePeriph, proxyPSM);
  await listenAndProxySOCKS(proxyChanStream);
}

Future<void> advertiseProxyPSM(BlePeripheral blePeriph, int psm) async {
  print('advertising self ($deviceName) and publishing SOCKS5 proxy PSM: $psm');
  await blePeriph
      .addReadOnlyService(viamSvcUUID, {viamSocksProxyPSMCharUUID: '$psm'});
  await blePeriph.startAdvertising(deviceName);
}

Future<void> listenAndProxySOCKS(Stream<L2CapChannel> chanStream) async {
  var chanCount = 0;
  print('waiting for new L2CAP connections to proxy');

  chanStream.listen((chan) async {
    final thisCount = chanCount++;
    print('serve channel $thisCount as a SOCKS5 server');
    final socksServerProxy = SocksServer();
    socksServerProxy.connections.listen((connection) async {
      print(
          'forwarding ${connection.address.address}:${connection.port} -> ${connection.desiredAddress.address}:${connection.desiredPort}');
      await connection.forward(allowIPv6: true);
    }).onError(print);

    unawaited(socksServerProxy
        .addServerSocket(L2CapChannelServerSocketUtils.multiplex(chan)));
  }).asFuture();
}

Future<void> manageMachine(BleCentral bleCentral, String machineName) async {
  print('scanning for $machineName now');
  late StreamSubscription<DiscoveredBlePeripheral> deviceSub;
  deviceSub = bleCentral.scanForPeripherals([viamSvcUUID]).listen(
    (periphInfo) {
      if (periphInfo.name == machineName) {
        print('found $machineName; connecting...');
        deviceSub.cancel();
      } else {
        return;
      }
      bleCentral.connectToPeripheral(periphInfo.id).then((periph) async {
        print('connected to machine');

        final char = periph.services
            .cast<BleService?>()
            .firstWhere((svc) => svc!.id == viamSvcUUID, orElse: () => null)
            ?.characteristics
            .cast<BleCharacteristic?>()
            .firstWhere((char) => char!.id == viamManagedMachinePSMCharUUID);
        if (char == null) {
          print('did not find needed PSM char after discovery');
          await Future<void>.delayed(const Duration(seconds: 1));
          print('disconnecting from machine and trying again');
          await periph.disconnect();
          unawaited(manageMachine(bleCentral, machineName));
          return;
        }

        Uint8List? val;
        try {
          val = await char.read();
        } catch (error) {
          print(
              'error reading characteristic $error; disconnecting from machine and trying again');
          await periph.disconnect();
          unawaited(manageMachine(bleCentral, machineName));
          return;
        }
        final psm = int.parse(utf8.decode(val!));
        print('will connect to management channel on psm: $psm');

        final L2CapChannel chan;
        try {
          chan = await periph.connectToL2CapChannel(psm);
          print('connected');
        } catch (error) {
          print(
              'error connecting $error; disconnecting from machine and trying again');
          await periph.disconnect();
          unawaited(manageMachine(bleCentral, machineName));
          return;
        }

        var successful = false;
        try {
          // all we will write is a length prefix encoded string that is our device name
          final encodedDeviceName = Uint8List(1 + deviceName.length);
          encodedDeviceName.buffer.asByteData().setUint8(0, deviceName.length);
          encodedDeviceName.setAll(1, deviceName.codeUnits);
          await chan.write(encodedDeviceName);
          print('wrote our device name ($deviceName)');
          successful = true;

          // give the device some time to process
          await Future<void>.delayed(const Duration(seconds: 5));
        } catch (error) {
          print(
              'error writing our device name: $error; disconnecting from machine and trying again');
        } finally {
          await chan.close();
          await periph.disconnect();
          if (!successful) {
            unawaited(manageMachine(bleCentral, machineName));
          }
        }
      }).catchError((error) {
        print('error connecting $error; will try again');
        unawaited(manageMachine(bleCentral, machineName));
      });
    },
    onError: (Object e) => print('connectAndTalk failed: $e'),
  );
}

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
        ),
        body: ListView.builder(
            itemCount: lines.length,
            itemBuilder: (BuildContext context, int index) {
              return SizedBox(
                child: Center(child: Text('Entry ${lines[index]}')),
              );
            }));
  }

  Future<void> loadData() async {
    while (true) {
      await Future<void>.delayed(const Duration(seconds: 1));
      setState(() {});
    }
  }
}
