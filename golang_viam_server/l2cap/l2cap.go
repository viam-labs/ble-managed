// Package l2cap defines an L2CAP socket.
package l2cap

// #cgo CFLAGS: -g -Wall
// #cgo LDFLAGS: -lbluetooth
// #include <stdlib.h>
// #include "l2cap.h"
import "C"
import (
	"fmt"
	"tinygo.org/x/bluetooth"
	"unsafe"
)

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
func (s *L2CAPSocket) Read() (string, error) {
	cSocket := C.int(*s)
	cMessage := C.CString("")
	if err := C.l2cap_read(cSocket, cMessage); err != 0 {
		return "", fmt.Errorf("error reading")
	}
	return C.GoString(cMessage), nil
}

// Close closes the L2CAP socket.
func (s *L2CAPSocket) Close() error {
	cSocket := C.int(*s)
	if err := C.l2cap_close(cSocket); err != 0 {
		return fmt.Errorf("error closing")
	}
	return nil
}
