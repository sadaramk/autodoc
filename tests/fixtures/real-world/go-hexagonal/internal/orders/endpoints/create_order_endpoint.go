// Package endpoints exposes the orders API over Echo.
package endpoints

import (
	"net/http"

	"github.com/acme/workouts/internal/orders/commands"
	"github.com/labstack/echo/v4"
	"github.com/mehdihadeli/go-mediatr"
)

// CreateOrderEndpoint serves POST /orders.
type CreateOrderEndpoint struct {
	group *echo.Group
}

// MapEndpoint registers the route.
func (ep *CreateOrderEndpoint) MapEndpoint() {
	ep.group.POST("/orders", ep.handler())
}

// handler dispatches the create-order command on the bus.
func (ep *CreateOrderEndpoint) handler() echo.HandlerFunc {
	return func(c echo.Context) error {
		command := commands.NewCreateOrder()
		result, err := mediatr.Send[*commands.CreateOrder, *commands.CreateOrderResponse](c.Request().Context(), command)
		if err != nil {
			return err
		}
		return c.JSON(http.StatusCreated, result)
	}
}
