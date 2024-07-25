package main

import (
	"context"
	"fmt"
	"io"
	"net"
	"net/http"
	"time"

	"golang.org/x/net/proxy"
	//"time"
	//apppb "go.viam.com/api/app/v1"
	//commonpb "go.viam.com/api/common/v1"
	//"go.viam.com/rdk/logging"
	//"go.viam.com/utils/rpc"
	//"google.golang.org/protobuf/types/known/timestamppb"
)

// Test SOCKS traffic by trying to send logs for a robot up to app.viam.com
// using a chained SOCKS dialer. Run the grill proxy on port 5000, run the
// mobile device proxy nearby, and run this program. If a log makes it to
// app.viam.com, the run was successful.

// curl https://storage.googleapis.com/packages.viam.com/apps/viam-server/viam-server-stable-aarch64.AppImage -o viam-server && chmod 755 viam-server && sudo ./viam-server --aix-install

//func main() {
//ctx := context.Background()
//logger := logging.NewDebugLogger("client")

//dialOpts := make([]rpc.DialOption, 0, 2)
//dialOpts = append(dialOpts, rpc.WithEntityCredentials(
//"b368c5d1-d3b3-464c-8d44-42f8d1c7df67",
//rpc.Credentials{
//Type:    rpc.CredentialsTypeAPIKey,
//Payload: "sjiibmj1c3av7wkrmsw43j1fz7ud9hyq",
//},
//), rpc.WithDialDebug())

//logger.Info("Creating gRPC connection to app.viam.com:443...")
//clientConn, err := rpc.DialDirectGRPC(ctx, "app.viam.com:443", logger, dialOpts...)
//if err != nil {
//panic(err)
//}
//logger.Info("Created gRPC connection to app.viam.com:443")

//logger.Info("Sending log to app.viam.com")
//client := apppb.NewRobotServiceClient(clientConn)
//log := &commonpb.LogEntry{
//Host:       "ble-managed",
//Level:      "info",
//Time:       timestamppb.New(time.Now()),
//LoggerName: "ble-managed",
//Message:    "hello world",
//}
//resp, err := client.Log(ctx, &apppb.LogRequest{Id: "c06196f9-f00b-43db-b41b-24181679eebf",
//Logs: []*commonpb.LogEntry{log}})
//if err != nil {
//panic(err)
//}
//logger.Infow("Successfully sent LogRequest to app; check app.viam.com", "resp", resp)
//}

/* Basic HTTP go client below */

func main() {
	proxyAddr := "localhost:5000"
	dialer, err := proxy.SOCKS5("tcp4", proxyAddr, nil, proxy.Direct)
	if err != nil {
		panic(fmt.Errorf("error dialing SOCKS proxy %q from environment: %w", proxyAddr, err))
	}

	addr := "https://storage.googleapis.com/packages.viam.com/apps/viam-server/viam-server-stable-aarch64.AppImage"

	transport := http.DefaultTransport.(*http.Transport).Clone()
	transport.DialContext = func(ctx context.Context, network string, addr string) (net.Conn, error) {
		println("GO CLIENT: Actually dialing...")
		conn, err := dialer.Dial(network, addr)
		if err != nil {
			println("GO CLIENT: Error from dialing", err.Error())
			return nil, err
		}
		return conn, nil
	}
	client := &http.Client{Transport: transport}

	// Getting!
	start := time.Now()
	println("GOUTILS: Getting...")
	resp, err := client.Get(addr)
	if err != nil {
		panic(err)
	}
	println("GOUTILS: Reading body...")
	_, err = io.ReadAll(resp.Body)
	if err != nil {
		panic(err)
	}
	finish := time.Since(start)
	fmt.Printf("GO CLIENT: Success getting viam server; took %v\n", finish.Seconds())

	println("GO CLIENT: finishing")
}
