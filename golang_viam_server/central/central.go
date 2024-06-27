// Package central allows starting and connecting centrals.
package central

import "C"
import (
	"ble-socks/l2cap"
	"context"
	"fmt"
	"log"
	"strconv"
	"time"

	"tinygo.org/x/bluetooth"
)

// Central is a central device.
type Central struct {
	adapter *bluetooth.Adapter
	socket  *l2cap.L2CAPSocket
}

// NewCentral makes a new central.
func NewCentral() *Central {
	return &Central{adapter: bluetooth.DefaultAdapter}
}

// Connnect opens an L2CAP CoC to a device:
// - named deviceName
// - with a service with the svcUUID provided
// - with a characteristic with the psmCharUUID provided that contains a PSM value
func (c *Central) Connect(ctx context.Context, deviceName string, svcUUID,
	psmCharUUID bluetooth.UUID) error {
	// Enable BLE interface.
	if err := c.adapter.Enable(); err != nil {
		return err
	}

	// Start scanning.
	log.Println("Scanning...")
	resultCh := make(chan bluetooth.ScanResult, 1)
	err := c.adapter.Scan(func(adapter *bluetooth.Adapter, result bluetooth.ScanResult) {
		log.Printf("Found device; address %s, RSSI: %v, name: %s\n", result.Address, result.RSSI, result.LocalName())
		if result.LocalName() != deviceName {
			return
		}

		if !result.HasServiceUUID(svcUUID) {
			log.Fatalf("Device %q is not advertising desired service UUID\n", result.LocalName())
		}
		log.Println("Device is target device; attempting to connect...")
		adapter.StopScan()
		resultCh <- result
	})
	if err != nil {
		return err
	}

	var result bluetooth.ScanResult
	select {
	case result = <-resultCh:
	case <-ctx.Done():
		return ctx.Err()
	}

	log.Println("Connecting to", result.Address.String(), "...")
	device, err := c.adapter.Connect(result.Address, bluetooth.ConnectionParams{})
	if err != nil {
		return err
	}

	log.Println("Connected to", result.Address.String())

	// Find current PSM value under specified service and PSM characteristic.
	log.Println("Fetching PSM value...")
	svcs, err := device.DiscoverServices([]bluetooth.UUID{svcUUID})
	if err != nil {
		return err
	}

	var targetSVC bluetooth.DeviceService
	for _, svc := range svcs {
		if svc.UUID() == svcUUID {
			targetSVC = svc
			break
		}
	}

	chars, err := targetSVC.DiscoverCharacteristics([]bluetooth.UUID{psmCharUUID})
	if err != nil {
		return err
	}

	var targetPSMChar bluetooth.DeviceCharacteristic
	for _, char := range chars {
		if char.UUID() == psmCharUUID {
			targetPSMChar = char
			break
		}
	}

	buf := make([]byte, 255)
	n, err := targetPSMChar.Read(buf)
	if err != nil {
		return err
	}

	// Pass PSM and address to underlying C library to connect.
	psm, err := strconv.ParseUint(string(buf[:n]), 10, 64)
	if err != nil {
		return err
	}
	log.Println("Found PSM of", psm)

	// Disconnect GATT layer.
	if err := device.Disconnect(); err != nil {
		return err
	}

	// TODO: Understand how to open an L2CAP connection to an already paired
	// device.
	log.Println("Sleeping in time for you to disconnect BT connection to phone")
	time.Sleep(20 * time.Second)

	log.Println("Opening L2CAP CoC to", device.Address.String(), " on PSM", psm)
	if c.socket, err = l2cap.OpenL2CAPCoc(device.Address, psm); err != nil {
		return err
	}
	return nil
}

// Write writes a message to the underlying socket.
func (c *Central) Write(message string) error {
	if c.socket == nil {
		return fmt.Errorf("central not connected")
	}
	return c.socket.Write(message)
}

// Read reads a message from the underlying socket.
func (c *Central) Read() (string, error) {
	if c.socket == nil {
		return "", fmt.Errorf("central not connected")
	}
	return c.socket.Read()
}

// Close closes the underlying socket.
func (c *Central) Close() error {
	if c.socket == nil {
		return fmt.Errorf("central not connected")
	}
	return c.socket.Close()
}
