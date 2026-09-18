// Package models holds the GORM models.
package models

import "gorm.io/gorm"

// OrderStatus is the lifecycle of an order.
type OrderStatus string

const (
	OrderNew      OrderStatus = "new"
	OrderPaid     OrderStatus = "paid"
	OrderRefunded OrderStatus = "refunded"
)

// Customer places orders.
type Customer struct {
	ID     uint   `gorm:"primaryKey"`
	Email  string `gorm:"size:255;uniqueIndex"`
	Orders []Order
}

// TableName keeps customers apart from other apps' tables.
func (Customer) TableName() string {
	return "customers_go"
}

// Order is a customer's purchase.
type Order struct {
	gorm.Model
	CustomerID uint
	Customer   Customer
	Status     OrderStatus `gorm:"type:text;default:'new'"`
	Total      int64       `gorm:"check:total > 0"`
	Note       *string
}
