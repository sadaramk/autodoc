// Package commands holds the order write side.
package commands

import (
	"context"

	"cloud.google.com/go/firestore"
)

// CreateOrder is the command payload.
type CreateOrder struct {
	OrderID string
}

// CreateOrderResponse is what the handler returns.
type CreateOrderResponse struct {
	OrderID string
}

// NewCreateOrder builds the command.
func NewCreateOrder() *CreateOrder {
	return &CreateOrder{}
}

// CreateOrderHandler handles CreateOrder.
type CreateOrderHandler struct {
	client *firestore.Client
}

// Handle stores the new order.
func (c *CreateOrderHandler) Handle(ctx context.Context, command *CreateOrder) (*CreateOrderResponse, error) {
	_, err := c.client.Collection("orders").Doc(command.OrderID).Set(ctx, command)
	return &CreateOrderResponse{OrderID: command.OrderID}, err
}
