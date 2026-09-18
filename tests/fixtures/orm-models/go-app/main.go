package main

import (
	"net/http"

	"example.com/ormgo/store"
)

func main() {
	s := store.New()
	http.HandleFunc("/pay", func(w http.ResponseWriter, r *http.Request) { _ = s.Pay(1) })
	_ = http.ListenAndServe(":8080", nil)
}
