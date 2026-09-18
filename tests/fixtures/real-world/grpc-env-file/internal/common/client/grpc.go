// Package client builds connections to other gym services.
package client

import (
	"errors"
	"os"
)

// TrainerClient talks to the trainer service over gRPC.
type TrainerClient struct {
	Addr string
}

// NewTrainerClient connects to the trainer service.
func NewTrainerClient() (*TrainerClient, error) {
	addr := os.Getenv("TRAINER_GRPC_ADDR")
	if addr == "" {
		return nil, errors.New("empty trainer address")
	}
	return &TrainerClient{Addr: addr}, nil
}

// Version reports the client library version.
func Version() string {
	return "1.0.0"
}
