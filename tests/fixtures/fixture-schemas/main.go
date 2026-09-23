package main

import "net/http"

// ListItems serves the catalogue.
func ListItems(w http.ResponseWriter, r *http.Request) {
	w.WriteHeader(http.StatusOK)
}

func main() {
	http.HandleFunc("GET /items", ListItems)
	http.ListenAndServe(":8080", nil)
}
