// Command trainings schedules trainings.
package main

import (
	"log"
	"net/http"

	"example.com/gym/common/client"
)

// main connects to the trainer and serves trainings.
func main() {
	trainer, err := client.NewTrainerClient()
	if err != nil {
		log.Fatal(err)
	}
	log.Println("trainer at", trainer.Addr)
	log.Fatal(http.ListenAndServe(":3000", nil))
}
