// Command trainer serves trainer schedules.
package main

import (
	"log"
	"net/http"

	"example.com/gym/common/client"
)

// main starts the trainer HTTP server.
func main() {
	log.Println("client", client.Version())
	log.Fatal(http.ListenAndServe(":3000", nil))
}
