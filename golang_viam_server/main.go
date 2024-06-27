// Package main contains example code of initial Viam-BLE connection logic.
package main

import (
	"ble-socks/central"
	"ble-socks/peripheral"
	"context"
	"log"
	"os"
	"os/signal"
	"syscall"

	"tinygo.org/x/bluetooth"
)

var (
	// Device name to advertise.
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
	log.Println("Starting main function.")
	ctx, ctxCancel := context.WithCancel(context.Background())
	sigs := make(chan os.Signal, 1)
	signal.Notify(sigs, syscall.SIGINT, syscall.SIGTERM)
	go func() {
		<-sigs
		ctxCancel()
	}()

	// Act as peripheral to receive proxy device name.
	periph := peripheral.NewPeripheral(ctx)
	err := periph.Advertise(managedMachineName, viamSVCUUID, viamSOCKSProxyMachineNameCharUUID)
	proxyDeviceName, err := periph.ProxyDeviceName()
	must("find proxy device name", err)
	log.Printf("Found proxy device name %q\n", proxyDeviceName)
	must("stop advertising", periph.StopAdvertise())

	// Connect to received proxy device name.
	cent := central.NewCentral()
	err = cent.Connect(ctx, proxyDeviceName, viamSVCUUID, viamSOCKSProxyMachinePSMCharUUID)
	must("connect", err)
	log.Println("Successfully connected.")
	defer func() {
		must("close connection", cent.Close())
	}()

	// Write to device.
	must("write", cent.Write("hello!"))
	log.Println("Successfully wrote.")

	// Read from device.
	message, err := cent.Read()
	must("read", err)
	log.Println("Successfully read message:", message)

	log.Println("Finished main function.")
}

func must(action string, err error) {
	if err != nil {
		log.Fatalln("Failed to " + action + ": " + err.Error())
	}
}
