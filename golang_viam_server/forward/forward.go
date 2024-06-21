// Package forward implements a forward dialer over BLE.
package forward

import (
	"ble-socks/central"
	"net"
	"time"

	"golang.org/x/net/proxy"
)

// BLEDialer is a forward dialer that uses BLE. To be passed in during SOCKS5
// dialer creation:
//
// bleDialer := forward.NewBLEDialer()
// socksDialer := proxy.SOCKS5("tcp", "google.com", proxy.Auth{...}, bleDialer)
type BLEDialer struct {
	conn *BLEConn
}

var _ proxy.Dialer = &BLEDialer{}

// NewBLEDialer creates a new BLE dialer.
func NewBLEDialer() *BLEDialer {
	return &BLEDialer{}
}

// Dial dials the address over the network and returns a BLE conn. Can return a
// cached BLE conn if one has already been made.
func (sbd *BLEDialer) Dial(network, addr string) (net.Conn, error) {
	if sbd.conn != nil {
		return sbd.conn, nil
	}

	bleConn := &BLEConn{central.NewCentral(nil)}

	// bleConn.Connect(...)
	// bleConn.Write("network and addr are as follows")
	// bleConn.Read(...) success?

	return bleConn, nil
}

// BLEConn implements net.Conn over BLE.
type BLEConn struct {
	*central.Central
}

// Read reads data from the connection.
// Read can be made to time out and return an error after a fixed
// time limit; see SetDeadline and SetReadDeadline.
func (bc *BLEConn) Read(b []byte) (int, error) {
	return 0, nil
}

// Write writes data to the connection.
// Write can be made to time out and return an error after a fixed
// time limit; see SetDeadline and SetWriteDeadline.
func (bc *BLEConn) Write(b []byte) (n int, err error) {
	return 0, nil
}

// Close closes the connection.
// Any blocked Read or Write operations will be unblocked and return errors.
func (bc *BLEConn) Close() error {
	return nil
}

// LocalAddr returns the local network address, if known.
func (bc *BLEConn) LocalAddr() net.Addr {
	return nil
}

// RemoteAddr returns the remote network address, if known.
func (bc *BLEConn) RemoteAddr() net.Addr {
	return nil
}

// SetDeadline sets the read and write deadlines associated
// with the connection. It is equivalent to calling both
// SetReadDeadline and SetWriteDeadline.
//
// A deadline is an absolute time after which I/O operations
// fail instead of blocking. The deadline applies to all future
// and pending I/O, not just the immediately following call to
// Read or Write. After a deadline has been exceeded, the
// connection can be refreshed by setting a deadline in the future.
//
// If the deadline is exceeded a call to Read or Write or to other
// I/O methods will return an error that wraps os.ErrDeadlineExceeded.
// This can be tested using errors.Is(err, os.ErrDeadlineExceeded).
// The error's Timeout method will return true, but note that there
// are other possible errors for which the Timeout method will
// return true even if the deadline has not been exceeded.
//
// An idle timeout can be implemented by repeatedly extending
// the deadline after successful Read or Write calls.
//
// A zero value for t means I/O operations will not time out.
func (bc *BLEConn) SetDeadline(t time.Time) error {
	return nil
}

// SetReadDeadline sets the deadline for future Read calls
// and any currently-blocked Read call.
// A zero value for t means Read will not time out.
func (bc *BLEConn) SetReadDeadline(t time.Time) error {
	return nil
}

// SetWriteDeadline sets the deadline for future Write calls
// and any currently-blocked Write call.
// Even if write times out, it may return n > 0, indicating that
// some of the data was successfully written.
// A zero value for t means Write will not time out.
func (bc *BLEConn) SetWriteDeadline(t time.Time) error {
	return nil
}
