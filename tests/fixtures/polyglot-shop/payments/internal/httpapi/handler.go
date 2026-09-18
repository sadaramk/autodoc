// Package httpapi exposes the payments service over HTTP.
package httpapi

import (
	"encoding/json"
	"net/http"

	"github.com/acme/shop/payments/internal/charge"
	"github.com/acme/shop/payments/internal/ledger"
)

// chargeRequest is the body of POST /charges.
type chargeRequest struct {
	// OrderID identifies the order being paid.
	OrderID     string `json:"orderId" validate:"required,uuid"`
	AmountCents int64  `json:"amountCents" validate:"required,gt=0"`
	Currency    string `json:"currency,omitempty" validate:"omitempty,oneof=usd eur gbp"`
	Token       string `json:"token" validate:"required"`
}

// chargeResponse reports the outcome of a charge attempt.
type chargeResponse struct {
	ChargeID string `json:"chargeId"`
	Status   string `json:"status"`
}

// Handler serves POST /charges.
type Handler struct {
	charger *charge.StripeCharger
	ledger  *ledger.Ledger
}

// NewHandler builds a Handler from its collaborators.
func NewHandler(c *charge.StripeCharger, l *ledger.Ledger) *Handler {
	return &Handler{charger: c, ledger: l}
}

// Routes returns the service mux.
func (h *Handler) Routes() http.Handler {
	mux := http.NewServeMux()
	mux.HandleFunc("POST /charges", h.createCharge)
	return mux
}

// createCharge charges the card through Stripe and records the attempt in the ledger.
func (h *Handler) createCharge(w http.ResponseWriter, r *http.Request) {
	var req chargeRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		http.Error(w, "bad request", http.StatusBadRequest)
		return
	}
	if req.OrderID == "" || req.AmountCents <= 0 {
		http.Error(w, "orderId and a positive amountCents are required", http.StatusUnprocessableEntity)
		return
	}
	res, err := h.charger.Charge(req.OrderID, req.AmountCents, req.Token)
	_ = h.ledger.Record(r.Context(), req.OrderID, res.ChargeID, res.Status, req.AmountCents)
	if err != nil {
		w.WriteHeader(http.StatusPaymentRequired)
	} else {
		w.WriteHeader(http.StatusCreated)
	}
	_ = json.NewEncoder(w).Encode(chargeResponse{ChargeID: res.ChargeID, Status: res.Status})
}
