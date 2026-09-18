// Package charge talks to Stripe to capture card payments.
package charge

import (
	"github.com/stripe/stripe-go/v79"
	"github.com/stripe/stripe-go/v79/paymentintent"
)

// Result is the outcome of a single charge attempt.
type Result struct {
	ChargeID string
	Status   string
}

// StripeCharger captures payments using the Stripe PaymentIntents API.
type StripeCharger struct{}

// NewStripeCharger configures the global Stripe key and returns a charger.
func NewStripeCharger(apiKey string) *StripeCharger {
	stripe.Key = apiKey
	return &StripeCharger{}
}

// Charge confirms a PaymentIntent for amountCents using the client token.
func (c *StripeCharger) Charge(orderID string, amountCents int64, token string) (Result, error) {
	params := &stripe.PaymentIntentParams{
		Amount:        stripe.Int64(amountCents),
		Currency:      stripe.String(string(stripe.CurrencyUSD)),
		PaymentMethod: stripe.String(token),
		Confirm:       stripe.Bool(true),
	}
	params.AddMetadata("order_id", orderID)
	pi, err := paymentintent.New(params)
	if err != nil {
		return Result{Status: "failed"}, err
	}
	return Result{ChargeID: pi.ID, Status: string(pi.Status)}, nil
}
