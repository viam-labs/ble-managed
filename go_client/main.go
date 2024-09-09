package main

import (
	"context"

	"go.viam.com/rdk/logging"
	"go.viam.com/rdk/robot/client"
	"go.viam.com/utils/rpc"
)

func main() {
	logger := logging.NewDebugLogger("client")
	machine, err := client.New(
		context.Background(),
		"rock4-main.h9kzybd9wn.viam.cloud",
		logger,
		client.WithDialOptions(rpc.WithEntityCredentials(
			"f2fd1149-99e7-40e1-9522-451cf69751c4",
			rpc.Credentials{
				Type:    rpc.CredentialsTypeAPIKey,
				Payload: "1550u387y911biq6eveob29u66llkfcl",
			}), rpc.WithDialDebug()),
	)
	if err != nil {
		logger.Fatal(err)
	}

	defer machine.Close(context.Background())
}
