// Package main contains example code to open an l2cap channel to a Viam device.
package main

import (
	"ble-socks/central"
	"context"
	"log"

	"tinygo.org/x/bluetooth"
)

func main() {
	log.Println("Starting main function.")

	viamSVCUUID, err := bluetooth.ParseUUID("00000000-0000-1234-0001-000000000000")
	must("parse service ID", err)
	viamPSMCharUUID, err := bluetooth.ParseUUID("00000000-0000-1234-0001-000000000001")
	must("parse characteristic ID", err)

	viamDeviceName := "TestBT1"

	central := central.NewCentral()
	err = central.Connect(context.Background(), viamSVCUUID, viamPSMCharUUID, viamDeviceName)
	must("connect", err)
	log.Println("Successfully connected.")

	err = central.Write("hello")
	must("write", err)
	log.Println("Successfully wrote.")

	log.Println("Finished main function.")

	err = central.Read()
	must("read", err)
	log.Println("Successfully read.")
}

func must(action string, err error) {
	if err != nil {
		log.Fatalln("failed to " + action + ": " + err.Error())
	}
}
