// Package peripheral contains peripheral logic.
package peripheral

import (
	"context"
	"fmt"
	"log"

	"tinygo.org/x/bluetooth"
)

// Peripheral is a peripheral device.
type Peripheral struct {
	adapter *bluetooth.Adapter
	adv     *bluetooth.Advertisement
}

// NewPeripheral makes a new peripheral.
func NewPeripheral() *Peripheral {
	return &Peripheral{adapter: bluetooth.DefaultAdapter}
}

// AdvertiseAndFindMobileDevice finds a mobile device name by advertising:
// - named deviceName
// - a service with the svcUUID provided
// - a characteristic with the psmCharUUID provided that contains a PSM value
func (p *Peripheral) AdvertiseAndFindMobileDevice(ctx context.Context, deviceName string,
	svcUUID, psmCharUUID bluetooth.UUID) (string, error) {
	// Enable BLE interface.
	if err := p.adapter.Enable(); err != nil {
		return "", err
	}

	// Define the peripheral device info.
	p.adv = p.adapter.DefaultAdvertisement()
	err := p.adv.Configure(bluetooth.AdvertisementOptions{
		LocalName: deviceName,
	})
	if err != nil {
		return "", err
	}

	// Start advertising
	log.Println("Advertising...")
	if err := p.adv.Start(); err != nil {
		return "", err
	}

	// TODO: finish.
	return "d3e535ca.viam.cloud", nil
}

// StopAdvertise stops advertising.
func (p *Peripheral) StopAdvertise() error {
	if p.adv != nil {
		return p.adv.Stop()
	}
	return fmt.Errorf("peripheral is not currently advertising")
}
