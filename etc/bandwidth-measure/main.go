package main

import (
	"fmt"
	"io"
	"net"
	"net/http"
	"time"

	"golang.org/x/net/proxy"
)

func main() {
	// Dial to SOCKS5 proxy (assumed to be at localhost:1080.)
	dialer, err := proxy.SOCKS5("tcp", "localhost:1080", nil, proxy.Direct)
	if err != nil {
		fmt.Println("Error creating SOCKS5 dialer:", err)
		return
	}

	// Create HTTP client over the connection.
	transport := &http.Transport{
		Dial: func(network, addr string) (net.Conn, error) {
			conn, err := dialer.Dial("tcp", addr)
			if err != nil {
				fmt.Println("Error creating SOCKS-proxied connection to google.com:", err.Error())
				return nil, err
			}
			return conn, nil
		},
	}
	client := &http.Client{
		Transport: transport,
	}

	var bandwidths []float64
	for i := 1; i <= 100; i++ {
		fmt.Printf("Running GET request %d\n...", i)

		startTime := time.Now()

		// GET from a random URL with no redirects.
		resp, err := client.Get("https://cscie93.dce.harvard.edu/fall2024/index.html")
		if err != nil {
			fmt.Println("Error performing GET request:", err)
			return
		}
		defer resp.Body.Close()

		// Measure the amount of data received.
		var totalBytes int64
		buffer := make([]byte, 1024)
		for {
			_, err := resp.Body.Read(buffer)
			if err != nil {
				if err == io.EOF {
					break
				}
				fmt.Println("Error reading response body:", err)
				return
			}
			totalBytes += int64(len(buffer))
		}

		duration := time.Since(startTime).Seconds()
		bandwidth := float64(totalBytes) / duration / float64(1024*1024)
		fmt.Printf("Received %d bytes in %.2f seconds. Bandwidth: %.6f MB/s\n", totalBytes, duration, bandwidth)
		bandwidths = append(bandwidths, bandwidth)
	}

	fmt.Printf("Average bandwidth: %.6f MB/s", average(bandwidths))
}

func average(nums []float64) float64 {
	if len(nums) == 0 {
		return 0
	}

	var sum float64
	for _, num := range nums {
		sum += num
	}
	return sum / float64(len(nums))
}
