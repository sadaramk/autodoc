package main

import (
	"log"
	"net/http"

	"example.com/tasks/internal/api"
)

func main() {
	log.Fatal(http.ListenAndServe(":8080", api.NewRouter()))
}
