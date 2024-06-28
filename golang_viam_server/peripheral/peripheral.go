// Package peripheral contains peripheral logic.
package peripheral

import (
	"context"
	"fmt"
	"log"
	"time"

	"tinygo.org/x/bluetooth"
)

// Peripheral is a peripheral device.
type Peripheral struct {
	adapter *bluetooth.Adapter
	adv     *bluetooth.Advertisement

	ctx                 context.Context
	ctxCancel           context.CancelFunc
	proxyDeviceNameChan chan string
}

// NewPeripheral makes a new peripheral.
func NewPeripheral(ctx context.Context) *Peripheral {
	p := &Peripheral{adapter: bluetooth.DefaultAdapter, proxyDeviceNameChan: make(chan string, 1)}
	p.ctx, p.ctxCancel = context.WithCancel(ctx)
	return p
}

// Advertise starts advertising a device:
//   - named deviceName
//   - with a service with the svcUUID provided
//   - with a characteristic with the proxyDeviceNameCharUUID provided that
//     should be written to by proxy devices
func (p *Peripheral) Advertise(deviceName string, svcUUID, proxyDeviceNameCharUUID bluetooth.UUID) error {
	if p.ctx.Err() != nil {
		return p.ctx.Err()
	}

	// Enable BLE interface.
	if err := p.adapter.Enable(); err != nil {
		return err
	}

	// Start advertising
	log.Printf("Advertising as %q...\n", deviceName)
	p.adv = p.adapter.DefaultAdvertisement()
	mde := bluetooth.ManufacturerDataElement{uint16(0xffff), []byte("empty")} // "testing" companyID.
	p.adv.Configure(bluetooth.AdvertisementOptions{
		LocalName: deviceName,
		//ServiceUUIDs:     []bluetooth.UUID{svcUUID},
		Interval:         bluetooth.NewDuration(1280 * time.Millisecond),
		ManufacturerData: []bluetooth.ManufacturerDataElement{mde},
	})
	if err := p.adv.Start(); err != nil {
		return err
	}

	// Add the proxy device name service/characteristic.
	log.Println("Adding service...")
	p.adapter.AddService(&bluetooth.Service{
		UUID: svcUUID,
		Characteristics: []bluetooth.CharacteristicConfig{
			{
				UUID: proxyDeviceNameCharUUID,
				Flags: bluetooth.CharacteristicReadPermission |
					bluetooth.CharacteristicWritePermission |
					bluetooth.CharacteristicWriteWithoutResponsePermission,
				WriteEvent: func(client bluetooth.Connection, offset int, value []byte) {
					if offset != 0 { // needed?
						return
					}
					select {
					case p.proxyDeviceNameChan <- string(value):
					case <-p.ctx.Done():
						log.Println("Halting write to proxy device name char...")
					}
				},
			},
		},
	})
	return nil
}

// StopAdvertise stops advertising.
func (p *Peripheral) StopAdvertise() error {
	log.Println("Stopping advertising...")
	if p.adv != nil {
		p.ctxCancel()
		close(p.proxyDeviceNameChan)
		return p.adv.Stop()
	}
	return fmt.Errorf("peripheral is not currently advertising")
}

// ProxyDeviceName waits for a proxy device name to be written to the
// advertised characteristic and returns it.
func (p *Peripheral) ProxyDeviceName() (string, error) {
	select {
	case proxyDeviceName := <-p.proxyDeviceNameChan:
		return proxyDeviceName, nil
	case <-p.ctx.Done():
		return "", p.ctx.Err()
	}
}
