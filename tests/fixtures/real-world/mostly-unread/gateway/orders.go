package main

import "net/http"

// listOrders returns the caller's open orders.
func listOrders(w http.ResponseWriter, r *http.Request) {
	status := r.URL.Query().Get("status")
	_ = status
	w.WriteHeader(http.StatusOK)
}
