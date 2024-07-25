package main

import (
	"fmt"
	"net/http"
)

// Basic go HTTP server that echos requests. For use in testing sending HTTP
// requests over the SOCKS bridge.

func echoHandler(w http.ResponseWriter, r *http.Request) {
	if r.Method == http.MethodGet {
		// Read the request URL and query parameters
		url := r.URL.String()
		query := r.URL.Query()

		println("Received GET request:\n")
		println("URL: %s\n", url)
		println("Query parameters: %v\n", query)

		// Echo back the request details
		fmt.Fprintf(w, "Received GET request:\n")
		fmt.Fprintf(w, "URL: %s\n", url)
		fmt.Fprintf(w, "Query parameters: %v\n", query)
	}
}

func main() {
	http.HandleFunc("/", echoHandler)
	port := 8080 // Change this to the desired port
	fmt.Printf("Listening on port %d...\n", port)
	http.ListenAndServe(fmt.Sprintf(":%d", port), nil)
}
