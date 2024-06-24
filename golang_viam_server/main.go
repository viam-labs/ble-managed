// Package main contains example code to open an l2cap channel to a Viam device.
package main

import (
	"ble-socks/central"
	"ble-socks/peripheral"
	"context"
	"log"

	"tinygo.org/x/bluetooth"
)

func main() {
	log.Println("Starting main function.")

	viamSVCUUID, err := bluetooth.ParseUUID("918ce61c-199f-419e-b6d5-59883a0049d8")
	must("parse service ID", err)
	viamSocksProxyMachinePSMCharUUID, err := bluetooth.ParseUUID("ab76ead2-b6e6-4f12-a053-61cd0eed19f9")
	must("parse characteristic ID", err)
	viamManagedMachinePSMCharUUID, err := bluetooth.ParseUUID("918ce61c-199f-419e-b6d5-59883a0049d8")
	must("parse characteristic ID", err)

	viamDeviceName := "mac1.loc1.viam.cloud"

	periph := peripheral.NewPeripheral()
	periph.Advertise(viamDeviceName, viamSVCUUID, viamManagedMachinePSMCharUUID)

	cent := central.NewCentral()
	err = cent.Connect(context.Background(), viamSVCUUID, viamSocksProxyMachinePSMCharUUID, viamDeviceName)
	must("connect", err)
	log.Println("Successfully connected.")

	err = cent.Write("hello")
	must("write", err)
	log.Println("Successfully wrote.")

	log.Println("Finished main function.")

	err = cent.Read()
	must("read", err)
	log.Println("Successfully read.")
}

func must(action string, err error) {
	if err != nil {
		log.Fatalln("failed to " + action + ": " + err.Error())
	}
}
