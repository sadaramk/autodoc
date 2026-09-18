// Command payments runs the payments HTTP service.
package main

import (
	"context"
	"log"
	"net/http"
	"os"

	"github.com/acme/shop/payments/internal/charge"
	"github.com/acme/shop/payments/internal/httpapi"
	"github.com/acme/shop/payments/internal/ledger"
	"github.com/jackc/pgx/v5/pgxpool"
)

// main wires Stripe, the Postgres ledger and the HTTP handler, then serves on :8080.
func main() {
	pool, err := pgxpool.New(context.Background(), os.Getenv("DATABASE_URL"))
	if err != nil {
		log.Fatalf("connect db: %v", err)
	}
	defer pool.Close()

	charger := charge.NewStripeCharger(os.Getenv("STRIPE_API_KEY"))
	book := ledger.New(pool)
	handler := httpapi.NewHandler(charger, book)

	log.Println("payments listening on :8080")
	log.Fatal(http.ListenAndServe(":8080", handler.Routes()))
}
