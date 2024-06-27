// Package main contains example code of initial Viam-BLE connection logic.
package main

import (
	"ble-socks/central"
	//"ble-socks/peripheral"
	"context"
	"log"

	"tinygo.org/x/bluetooth"
)

var (
	// Device name to advertise.
	managedMachineName = "mac1.loc1.viam.cloud"
	// Viam service UUID.
	viamSVCUUID bluetooth.UUID
	// PSM chararectistic UUID on the SOCKS proxy.
	viamSocksProxyMachinePSMCharUUID bluetooth.UUID
	// PSM chararectistic UUID on this (managed) machine.
	viamManagedMachinePSMCharUUID bluetooth.UUID
)

func init() {
	var err error
	viamSVCUUID, err = bluetooth.ParseUUID("79cf4eca-116a-4ded-8426-fb83e53bc1d7")
	must("parse service ID", err)
	viamSocksProxyMachinePSMCharUUID, err = bluetooth.ParseUUID("ab76ead2-b6e6-4f12-a053-61cd0eed19f9")
	must("parse socks proxy characteristic ID", err)
	viamManagedMachinePSMCharUUID, err = bluetooth.ParseUUID("918ce61c-199f-419e-b6d5-59883a0049d8")
	must("parse managed characteristic ID", err)
}

func main() {
	log.Println("Starting main function.")

	// Act as peripheral to receive device name.
	//periph := peripheral.NewPeripheral()
	//mobileDeviceName, err := periph.AdvertiseAndFindMobileDevice(context.Background(),
	//managedMachineName, viamSVCUUID, viamManagedMachinePSMCharUUID)

	// TODO: Remove hardcoding.
	mobileDeviceName := "d3e535ca.viam.cloud"

	// Connect to received device name.
	cent := central.NewCentral()
	err := cent.Connect(context.Background(), mobileDeviceName, viamSVCUUID,
		viamSocksProxyMachinePSMCharUUID)
	must("connect", err)
	log.Println("Successfully connected.")
	defer func() {
		if err := cent.Close(); err != nil {
			log.Printf("Error closing connection: %v\n", err)
		}
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
