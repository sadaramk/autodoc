// Package query holds the read side.
package query

import "context"

// Training is a read model.
type Training struct {
	UUID string
	User string
}

// AllTrainingsReadModel reads trainings.
type AllTrainingsReadModel interface {
	AllTrainings(ctx context.Context) ([]Training, error)
}

// AllTrainingsHandler answers the all-trainings query.
type AllTrainingsHandler struct {
	readModel AllTrainingsReadModel
}

// Handle returns every training.
func (h AllTrainingsHandler) Handle(ctx context.Context) ([]Training, error) {
	return h.readModel.AllTrainings(ctx)
}
