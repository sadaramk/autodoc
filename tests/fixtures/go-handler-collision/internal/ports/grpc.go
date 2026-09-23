package ports

import (
	"context"

	"github.com/golang/protobuf/ptypes/empty"
)

// GrpcServer serves the same operations to other services over gRPC.
type GrpcServer struct{}

// MakeHourAvailable is the gRPC method. It shares its name with the HTTP
// handler below, sorts earlier by filename, and takes a request message rather
// than an HTTP request — so citing it for an HTTP route is wrong twice: the
// citation points at the wrong function, and there is no body to read.
func (g GrpcServer) MakeHourAvailable(ctx context.Context, request *UpdateHourRequest) (*empty.Empty, error) {
	return &empty.Empty{}, nil
}
