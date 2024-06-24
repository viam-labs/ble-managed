// Package peripheral contains peripheral logic.
package peripheral

import (
	"fmt"
	"log"

	"tinygo.org/x/bluetooth"
)

type Peripheral struct {
	adapter *bluetooth.Adapter
	adv     *bluetooth.Advertisement
}

func NewPeripheral() *Peripheral {
	return &Peripheral{bluetooth.DefaultAdapter}
}

func (p *Peripheral) Advertise(deviceName string, svcUUID, psmCharUUID bluetooth.UUID) error {
	// Enable BLE interface.
	if err := p.adapter.Enable(); err != nil {
		return err
	}

	// Define the peripheral device info.
	adv := p.adapter.DefaultAdvertisement()

	sde := &bluetooth.ServiceDataElement{}
	bluetooth.Characteristic

	err := adv.Configure(bluetooth.AdvertisementOptions{
		LocalName: deviceName,
	})
	if err != nil {
		return err
	}

	// Start advertising
	if err := adv.Start(); err != nil {
		return err
	}

	p.adv = adv
	log.Println("Now advertising...")
	return nil
}

func (p *Peripheral) StopAdvertise() error {
	if p.adv != nil {
		return p.adv.Stop()
	}
	return fmt.Errorf("peripheral is not currently advertising")
}
