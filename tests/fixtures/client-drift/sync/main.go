// Mirrors accounts into the warehouse.
package main

import (
	"encoding/json"
	"io"
	"net/http"
)

// Account is the shape sync writes to the warehouse.
type Account struct {
	ID        string `json:"id"`
	Name      string `json:"name"`
	Balance   string `json:"balance"`
	LastLogin string `json:"lastLogin"`
}

func fetch(client *http.Client, id string) (Account, error) {
	var account Account
	resp, err := client.Get("http://accounts/accounts/" + id)
	if err != nil {
		return account, err
	}
	defer resp.Body.Close()
	body, err := io.ReadAll(resp.Body)
	if err != nil {
		return account, err
	}
	if err := json.Unmarshal(body, &account); err != nil {
		return account, err
	}
	return account, nil
}
