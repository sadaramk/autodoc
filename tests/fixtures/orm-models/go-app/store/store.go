// Package store persists orders.
package store

import (
	"gorm.io/gorm"

	"example.com/ormgo/models"
)

// Store wraps the database.
type Store struct {
	db *gorm.DB
}

// New opens the store.
func New() *Store {
	return &Store{}
}

// Place stores a new order.
func (s *Store) Place(customerID uint, total int64) error {
	order := models.Order{CustomerID: customerID, Total: total, Status: models.OrderNew}
	return s.db.Create(&order).Error
}

// Pay marks a new order paid.
func (s *Store) Pay(id uint) error {
	return s.db.Model(&models.Order{}).Where("id = ? AND status = ?", id, models.OrderNew).Update("status", models.OrderPaid).Error
}

// Refund refunds a paid order.
func (s *Store) Refund(id uint) error {
	var order models.Order
	if err := s.db.First(&order, id).Error; err != nil {
		return err
	}
	if order.Status == models.OrderPaid {
		order.Status = models.OrderRefunded
	}
	return s.db.Save(&order).Error
}
