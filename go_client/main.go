package main

import (
	"context"
	"time"

	apppb "go.viam.com/api/app/v1"
	commonpb "go.viam.com/api/common/v1"
	"go.viam.com/rdk/logging"
	"go.viam.com/utils/rpc"
	"google.golang.org/protobuf/types/known/timestamppb"
)

// Test SOCKS traffic by trying to send logs for a robot up to app.viam.com
// using a chained SOCKS dialer. Run the grill proxy on port 5000, run the
// mobile device proxy nearby, and run this program. If a log makes it to
// app.viam.com, the run was successful.

func main() {
	ctx := context.Background()
	logger := logging.NewDebugLogger("client")

	dialOpts := make([]rpc.DialOption, 0, 2)
	dialOpts = append(dialOpts, rpc.WithEntityCredentials(
		"b368c5d1-d3b3-464c-8d44-42f8d1c7df67",
		rpc.Credentials{
			Type:    rpc.CredentialsTypeAPIKey,
			Payload: "sjiibmj1c3av7wkrmsw43j1fz7ud9hyq",
		},
	), rpc.WithDialDebug())

	logger.Info("Creating gRPC connection to app.viam.com:443...")
	clientConn, err := rpc.DialDirectGRPC(ctx, "app.viam.com:443", logger, dialOpts...)
	if err != nil {
		panic(err)
	}
	logger.Info("Created gRPC connection to app.viam.com:443")

	logger.Info("Sending log to app.viam.com")
	client := apppb.NewRobotServiceClient(clientConn)
	log := &commonpb.LogEntry{
		Host:       "ble-managed",
		Level:      "info",
		Time:       timestamppb.New(time.Now()),
		LoggerName: "ble-managed",
		Message:    "hello world",
	}
	resp, err := client.Log(ctx, &apppb.LogRequest{Id: "c06196f9-f00b-43db-b41b-24181679eebf",
		Logs: []*commonpb.LogEntry{log}})
	if err != nil {
		panic(err)
	}
	logger.Infow("Successfully sent LogRequest to app; check app.viam.com", "resp", resp)
}
