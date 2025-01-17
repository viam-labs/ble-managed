package main

import (
	"fmt"
	"io"
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

	// Create connection to google.com
	conn, err := dialer.Dial("tcp", "google.com:80")
	if err != nil {
		fmt.Println("Error creating SOCKS-proxied connection to google.com:", err)
		return
	}
	defer conn.Close()

	var bandwidths []float64
	for i := range 100 {
		fmt.Printf("Running GET request %d\n...", i)
		// Send 100 simple HTTP GET requests.
		request := "GET / HTTP/1.1\r\nHost: google.com\r\n\r\n"
		conn.Write([]byte(request))

		// Measure bandwidth
		startTime := time.Now()
		var totalBytes int64

		buffer := make([]byte, 1024)
		for {
			n, err := conn.Read(buffer)
			if err != nil {
				if err == io.EOF {
					break
				}
				fmt.Println("Error reading from connection:", err)
				return
			}
			totalBytes += int64(n)
		}

		duration := time.Since(startTime).Seconds()
		bandwidth := float64(totalBytes) / duration / (1024 * 1024)
		bandwidths = append(bandwidths, bandwidth)
		fmt.Printf("Received %d bytes in %.2f seconds. Bandwidth: %.2f MB/s\n", totalBytes, duration, bandwidth)
	}

	fmt.Printf("Average bandwidth: %.2f MB/s", average(bandwidths))
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
