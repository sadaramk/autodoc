// Command urlshort is a tiny in-memory URL shortener.
package main

import (
	"log"
	"net/http"

	"example.com/urlshort/internal/handlers"
	"example.com/urlshort/internal/store"
)

// main starts the shortener on :8080.
func main() {
	s := store.NewMemory()
	h := handlers.New(s)
	log.Println("urlshort listening on :8080")
	log.Fatal(http.ListenAndServe(":8080", h.Routes()))
}
