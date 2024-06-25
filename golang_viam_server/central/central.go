// Package central allows starting and connecting centrals.
package central

// #cgo CFLAGS: -g -Wall
// #cgo LDFLAGS: -lbluetooth
// #include <stdlib.h>
// #include "l2cap.h"
import "C"
import (
	"context"
	"fmt"
	"log"
	"strconv"
	"time"
	"unsafe"

	"tinygo.org/x/bluetooth"
)

// Central is a central device.
type Central struct {
	adapter *bluetooth.Adapter

	socket *L2CAPSocket
}

// NewCentral makes a new central.
func NewCentral() *Central {
	return &Central{adapter: bluetooth.DefaultAdapter}
}

// Connnect opens an L2CAP CoC to a device with:
// - a service with the svcUUID provided
// - a characteristic with the psmCharUUID provided that contains a PSM value
// - named name
func (c *Central) Connect(ctx context.Context, svcUUID, psmCharUUID bluetooth.UUID, name string) error {
	// Enable BLE interface.
	if err := c.adapter.Enable(); err != nil {
		return err
	}

	// Start scanning.
	log.Println("Scanning...")
	resultCh := make(chan bluetooth.ScanResult, 1)
	err := c.adapter.Scan(func(adapter *bluetooth.Adapter, result bluetooth.ScanResult) {
		log.Printf("Found device; address %s, RSSI: %v, name: %s\n", result.Address, result.RSSI, result.LocalName())
		if result.LocalName() != name {
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
	if c.socket, err = OpenL2CAPCoc(device.Address, psm); err != nil {
		return err
	}
	log.Println("DEBUG: Returned socket number is", *c.socket)
	defer func() {
		c.socket.Close()
	}()

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
func (c *Central) Read() error {
	if c.socket == nil {
		return fmt.Errorf("central not connected")
	}
	c.socket.Read()
	return nil
}

func (c *Central) Close() error {
	if c.socket == nil {
		return fmt.Errorf("central not connected")
	}
	c.socket.Close()
	return nil
}

// L2CAPSocket is a light wrapper around an int representing a socket.
type L2CAPSocket int

// OpenL2CAPCoc opens a new L2CAP CoC against the provided address and PSM.
func OpenL2CAPCoc(addr bluetooth.Address, psm uint64) (*L2CAPSocket, error) {
	cAddr := C.CString(addr.String())
	defer C.free(unsafe.Pointer(cAddr))

	cPsm := C.uint(psm)
	socketPtr := C.malloc(C.sizeof_int)

	if err := C.l2cap_dial(cAddr, cPsm, (*C.int)(socketPtr)); err != 0 {
		return nil, fmt.Errorf("error connecting")
	}
	return (*L2CAPSocket)(socketPtr), nil
}

// Write writes a message to the L2CAP socket.
func (s *L2CAPSocket) Write(message string) error {
	cSocket := C.int(*s)
	cMessage := C.CString(message)
	defer C.free(unsafe.Pointer(cMessage))

	if err := C.l2cap_write(cSocket, cMessage); err != 0 {
		return fmt.Errorf("error writing")
	}
	return nil
}

// Read reads a message from the L2CAP socket.
func (s *L2CAPSocket) Read() {
	cSocket := C.int(*s)
	C.l2cap_read(cSocket)
}

// Close closes the L2CAP socket.
func (s *L2CAPSocket) Close() {
	cSocket := C.int(*s)
	C.l2cap_close(cSocket)
}
