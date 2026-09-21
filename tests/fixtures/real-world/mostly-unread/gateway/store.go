package main

import "database/sql"

// saveOrder writes an accepted order.
func saveOrder(db *sql.DB, symbol string, qty int) error {
	_, err := db.Exec("INSERT INTO orders (symbol, quantity) VALUES ($1, $2)", symbol, qty)
	return err
}
