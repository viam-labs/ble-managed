// ignore_for_file: avoid_print, public_member_api_docs
import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:ble/ble.dart';
import 'package:ble/ble_central.dart';
import 'package:ble/ble_peripheral.dart';
import 'package:ble/ble_socket.dart';
import 'package:flutter/material.dart';
import 'package:permission_handler/permission_handler.dart';
import 'package:socks5_proxy/socks.dart';

List<String> lines = [];

var machineName = 'mac1.loc1.viam.cloud';

const viamSvcUUID = '79cf4eca-116a-4ded-8426-fb83e53bc1d7';
const viamSocksProxyPSMCharUUID = 'ab76ead2-b6e6-4f12-a053-61cd0eed19f9';
const viamManagedMachinePSMCharUUID = '918ce61c-199f-419e-b6d5-59883a0049d8';

// This is an ephemeral example of a server that becomes managed and starts sending
// SOCKS5 requests.
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
              becomeManagedAndProxy(blePeriph);
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

Future<void> becomeManagedAndProxy(BlePeripheral blePeriph) async {
  final managerName = await becomeManaged(blePeriph);
  print("manager is $managerName");
  final bleCentral = await BleCentral.create();
  final stateStream = bleCentral.getState();
  late StreamSubscription<AdapterState> streamSub;
  streamSub = stateStream.listen((state) {
    if (state == AdapterState.poweredOn) {
      streamSub.cancel();
      connectToAndUseManagerProxy(bleCentral, managerName);
    }
  });
}

Future<String> becomeManaged(BlePeripheral blePeriph) async {
  final (proxyPSM, proxyChanStream) = await blePeriph.publishL2capChannel();
  await advertiseManagedPSM(blePeriph, proxyPSM);
  return waitForManager(proxyChanStream);
}

Future<void> advertiseManagedPSM(BlePeripheral blePeriph, int psm) async {
  print(
      'advertising self ($machineName) and publishing managed machine PSM: $psm');
  await blePeriph
      .addReadOnlyService(viamSvcUUID, {viamManagedMachinePSMCharUUID: '$psm'});
  await blePeriph.startAdvertising(machineName);
}

Future<String> waitForManager(Stream<L2CapChannel> chanStream) async {
  final chan = await chanStream.first;
  // we will read way less than this and based on min packet size will get our value
  final encodedDeviceName = (await chan.read(256))!;
  final length = encodedDeviceName[0];
  if (encodedDeviceName.length - 1 < length) {
    throw "bad device name length ${encodedDeviceName.length - 1}; expected $length";
  }
  return utf8.decode(encodedDeviceName.sublist(1, length + 1));
}

Future<void> connectToAndUseManagerProxy(
    BleCentral bleCentral, String managerName) async {
  print('scanning for manager ($managerName) now');
  late StreamSubscription<DiscoveredBlePeripheral> deviceSub;
  deviceSub = bleCentral.scanForPeripherals([viamSvcUUID]).listen(
    (periphInfo) {
      print("got some info ${periphInfo.name}");
      if (periphInfo.name == managerName) {
        print('found device; connecting...');
        deviceSub.cancel();
      } else {
        return;
      }
      bleCentral.connectToPeripheral(periphInfo.id).then((periph) async {
        print('connected to manager');

        final char = periph.services
            .cast<BleService?>()
            .firstWhere((svc) => svc!.id == viamSvcUUID, orElse: () => null)
            ?.characteristics
            .cast<BleCharacteristic?>()
            .firstWhere((char) => char!.id == viamSocksProxyPSMCharUUID);
        if (char == null) {
          print('did not find needed PSM char after discovery');
          await Future<void>.delayed(const Duration(seconds: 1));
          print('disconnecting from manager and trying again');
          await periph.disconnect();
          unawaited(connectToAndUseManagerProxy(bleCentral, managerName));
          return;
        }

        Uint8List? val;
        try {
          val = await char.read();
        } catch (error) {
          print(
              'error reading characteristic $error; disconnecting from manager and trying again');
          await periph.disconnect();
          unawaited(connectToAndUseManagerProxy(bleCentral, managerName));
          return;
        }
        final psm = int.parse(utf8.decode(val!));
        print('will connect to SOCKS proxy channel on psm: $psm');

        final L2CapChannel chan;
        try {
          chan = await periph.connectToL2CapChannel(psm);
          print('connected');
        } catch (error) {
          print(
              'error connecting $error; disconnecting from manager and trying again');
          await periph.disconnect();
          unawaited(connectToAndUseManagerProxy(bleCentral, managerName));
          return;
        }

        final socketMux = L2CapChannelClientSocketUtils.multiplex(chan);
        print('multiplexed the channel');

        try {
          while (true) {
            final List<Socket> connectedSockets = [];
            try {
              await IOOverrides.runZoned(() {
                return makeRequests();
              }, socketConnect: (host, int port,
                  {sourceAddress, int sourcePort = 0, Duration? timeout}) {
                final connectedSocket = socketMux.connectSocket();
                connectedSockets.add(connectedSocket);
                return Future.value(connectedSocket);
              });
            } catch (error) {
              print('error doing request $error');
              rethrow;
            } finally {
              for (var socket in connectedSockets) {
                await socket.close();
              }
            }
          }
        } finally {
          await chan.close();
          print('disconnecting from manager and trying again');
          await periph.disconnect();
          unawaited(connectToAndUseManagerProxy(bleCentral, managerName));
        }
      }).catchError((error) {
        print('error connecting $error; will try again');
        unawaited(connectToAndUseManagerProxy(bleCentral, managerName));
      });
    },
    onError: (Object e) => print('connectAndTalk failed: $e'),
  );
}

Future<void> makeRequests() async {
  final client = HttpClient();
  client.userAgent = 'curl/8.6.0';

  SocksTCPClient.assignToHttpClient(client, [
    ProxySettings(InternetAddress.loopbackIPv4, 1080),
  ]);

  const url = 'http://ifconfig.io';

  try {
    await Future<void>.delayed(const Duration(seconds: 4));
    for (var i = 0; i < 5; i++) {
      await Future<void>.delayed(const Duration(seconds: 1));
      final request = await client.getUrl(Uri.parse(url));
      final response = await request.close();
      final decoded = await utf8.decodeStream(response);
      print('got ${decoded.length} bytes');
      print(decoded);
    }
  } finally {
    client.close();
  }
}

class MyApp extends StatelessWidget {
  const MyApp({super.key});

  @override
  Widget build(BuildContext context) {
    return const MaterialApp(
        title: 'Fake Viam Server', home: MyHomePage(title: 'Fake Viam Server'));
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
