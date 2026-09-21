// Package main serves the trading gateway.
package main

import (
	"encoding/json"
	"net/http"
)

// orderRequest is the body of POST /orders.
type orderRequest struct {
	Symbol   string `json:"symbol" validate:"required"`
	Quantity int    `json:"quantity" validate:"gt=0"`
}

func main() {
	mux := http.NewServeMux()
	mux.HandleFunc("POST /orders", placeOrder)
	http.ListenAndServe(":8080", mux)
}

// placeOrder forwards an order to the matching engine.
func placeOrder(w http.ResponseWriter, r *http.Request) {
	var req orderRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		http.Error(w, "invalid order", http.StatusBadRequest)
		return
	}
	w.WriteHeader(http.StatusAccepted)
}
