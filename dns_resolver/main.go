package main

import (
	"context"
	"fmt"
	"net"
	"time"

	"golang.org/x/net/proxy"
)

func main() {
	address := "global.turn.twilio.com:3478"
	network := "tcp4"
	proxyAddr := "localhost:5000"

	proxyDialer, err := proxy.SOCKS5("tcp", proxyAddr, nil, proxy.Direct)
	if err != nil {
		fmt.Printf("Error creating SOCKS proxy dialer to address %q from environment: %w\n",
			proxyAddr, err)
		return
	}

	println("Resolving TCP address from custom transport for address", address,
		" on network", network)

	// Custom resolver to contact an external DNS server via the proxy dialer.
	resolver := &net.Resolver{
		PreferGo: false,
		Dial: func(_ context.Context, network, _ string) (net.Conn, error) {
			println("Proxy dialing for TCP-IP resolution from custom transport")
			return proxyDialer.Dial(network, "1.1.1.1:53") // hardcode external DNS
		},
	}

	ctx, cancel := context.WithTimeout(context.Background(), time.Second*5)
	defer cancel()

	println("Looking up host...")
	ips, err := resolver.LookupHost(ctx, address)
	if err != nil {
		println("Error: could not lookup host from custom transport", err.Error())
		return
	}

	// Take only first IP returned.
	if len(ips) > 0 {
		ip := ips[0]
		tcpAddr := &net.TCPAddr{IP: net.ParseIP(ip)} // leave port empty for now.
		println("Resolved TCP-IP address to", tcpAddr.String())
		return
	}

	println("Could not resolve IP address for", address, " on network", network)
}
