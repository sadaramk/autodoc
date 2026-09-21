package main

import "net/http"

// engineClient forwards matched orders to the C++ matching engine.
type engineClient struct{ base string }

func (c *engineClient) submit(symbol string, qty int) (*http.Response, error) {
	return http.Post(c.base+"/match", "application/json", nil)
}
