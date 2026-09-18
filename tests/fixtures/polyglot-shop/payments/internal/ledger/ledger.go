// Package ledger records every charge attempt in Postgres.
package ledger

import (
	"context"

	"github.com/jackc/pgx/v5/pgxpool"
)

// Ledger is an append-only record of payments.
type Ledger struct {
	pool *pgxpool.Pool
}

// New returns a Ledger backed by the given connection pool.
func New(pool *pgxpool.Pool) *Ledger {
	return &Ledger{pool: pool}
}

// Record appends a payment row for the order.
func (l *Ledger) Record(ctx context.Context, orderID, chargeID, status string, amountCents int64) error {
	_, err := l.pool.Exec(ctx,
		"INSERT INTO payments (order_id, charge_id, status, amount_cents) VALUES ($1, $2, $3, $4)",
		orderID, chargeID, status, amountCents)
	return err
}
