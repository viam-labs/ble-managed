// Package main contains example code to connect to a golang_viam_server as
// as a central and write a proxy device name to a characteristic.
package main

import (
	"context"
	"log"
	"os"
	"os/signal"
	"syscall"

	"tinygo.org/x/bluetooth"
)

var (
	// Name of proxy (this) machine.
	proxyMachineName = "d3e535ca.viam.cloud"
	// Name of machine to manage.
	managedMachineName = "mac1.loc1.viam.cloud"
	// Viam service UUID.
	viamSVCUUID bluetooth.UUID
	// PSM characteristic UUID on the SOCKS proxy.
	viamSOCKSProxyMachinePSMCharUUID bluetooth.UUID
	// Proxy device name characteristic UUID on this machine.
	viamSOCKSProxyMachineNameCharUUID bluetooth.UUID
)

func init() {
	var err error
	viamSVCUUID, err = bluetooth.ParseUUID("79cf4eca-116a-4ded-8426-fb83e53bc1d7")
	must("parse service ID", err)
	viamSOCKSProxyMachinePSMCharUUID, err = bluetooth.ParseUUID("ab76ead2-b6e6-4f12-a053-61cd0eed19f9")
	must("parse socks proxy characteristic ID", err)
	viamSOCKSProxyMachineNameCharUUID, err = bluetooth.ParseUUID("918ce61c-199f-419e-b6d5-59883a0049d8")
	must("parse managed characteristic ID", err)
}

func main() {
	ctx, ctxCancel := context.WithCancel(context.Background())
	sigs := make(chan os.Signal, 1)
	signal.Notify(sigs, syscall.SIGINT, syscall.SIGTERM)
	go func() {
		<-sigs
		ctxCancel()
	}()

	adapter := bluetooth.DefaultAdapter
	must("enable adapter", adapter.Enable())

	// Start scanning.
	log.Println("Scanning...")
	resultCh := make(chan bluetooth.ScanResult, 1)
	err := adapter.Scan(func(adapter *bluetooth.Adapter, result bluetooth.ScanResult) {
		if ctx.Err() != nil {
			log.Println("Stopping in-progress bluetooth scan...")
			adapter.StopScan()
		}

		log.Printf("Found device; address %s, RSSI: %v, name: %s\n", result.Address, result.RSSI, result.LocalName())
		if result.LocalName() != managedMachineName {
			return
		}

		if !result.HasServiceUUID(viamSVCUUID) {
			log.Fatalf("Device %q is not advertising desired service UUID\n", result.LocalName())
		}
		log.Println("Device is target device; attempting to connect...")
		adapter.StopScan()
		resultCh <- result
	})
	must("scan", err)

	var result bluetooth.ScanResult
	select {
	case result = <-resultCh:
	case <-ctx.Done():
		return
	}

	log.Println("Connecting to", result.Address.String(), "...")
	device, err := adapter.Connect(result.Address, bluetooth.ConnectionParams{})
	must("connect", err)

	log.Println("Writing to proxy device name characteristic...")
	svcs, err := device.DiscoverServices([]bluetooth.UUID{viamSVCUUID})
	must("discover services", err)

	var targetSVC bluetooth.DeviceService
	for _, svc := range svcs {
		if svc.UUID() == viamSVCUUID {
			targetSVC = svc
			break
		}
	}

	chars, err := targetSVC.DiscoverCharacteristics([]bluetooth.UUID{
		viamSOCKSProxyMachineNameCharUUID})
	must("discover characteristics", err)

	var targetProxyMachineNameChar bluetooth.DeviceCharacteristic
	for _, char := range chars {
		if char.UUID() == viamSOCKSProxyMachineNameCharUUID {
			targetProxyMachineNameChar = char
			break
		}
	}

	_, err = targetProxyMachineNameChar.WriteWithoutResponse([]byte(
		proxyMachineName))
	must("write to characteristic", err)

	log.Println("Successfully wrote to proxy device name characteristic")
}

func must(action string, err error) {
	if err != nil {
		log.Fatalln("Failed to " + action + ": " + err.Error())
	}
}
