// Package adapters implements the ports against Firestore.
package adapters

import (
	"context"

	"cloud.google.com/go/firestore"
	"github.com/acme/workouts/internal/trainings/app/command"
	"github.com/acme/workouts/internal/trainings/app/query"
)

// TrainingsFirestoreRepository implements the trainings port.
type TrainingsFirestoreRepository struct {
	client *firestore.Client
}

// AddTraining writes a training document.
func (r TrainingsFirestoreRepository) AddTraining(ctx context.Context, t command.ScheduleTraining) error {
	_, err := r.client.Collection("trainings").Doc(t.UUID).Set(ctx, t)
	return err
}

// AllTrainings reads every training document.
func (r TrainingsFirestoreRepository) AllTrainings(ctx context.Context) ([]query.Training, error) {
	docs, err := r.client.Collection("trainings").Documents(ctx).GetAll()
	if err != nil {
		return nil, err
	}
	out := make([]query.Training, 0, len(docs))
	return out, nil
}
