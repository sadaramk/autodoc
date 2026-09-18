package main

import (
	"database/sql"
	"log"
	"os"
	"time"

	_ "github.com/lib/pq"
)

// collect samples a gauge every minute.
func collect(db *sql.DB) {
	ticker := time.NewTicker(time.Minute)
	defer ticker.Stop()
	for range ticker.C {
		if _, err := db.Exec("INSERT INTO samples (value) VALUES ($1)", 1.0); err != nil {
			log.Println(err)
		}
	}
}

func main() {
	db, err := sql.Open("postgres", os.Getenv("DATABASE_URL"))
	if err != nil {
		log.Fatal(err)
	}
	collect(db)
}
