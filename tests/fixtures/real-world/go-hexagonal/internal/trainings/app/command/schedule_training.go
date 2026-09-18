// Package command holds the write side.
package command

import "context"

// ScheduleTraining is the command payload.
type ScheduleTraining struct {
	UUID string
	User string
}

// Repository stores trainings; the port implemented by the adapter.
type Repository interface {
	AddTraining(ctx context.Context, t ScheduleTraining) error
}

// ScheduleTrainingHandler schedules a training.
type ScheduleTrainingHandler struct {
	repo Repository
}

// Handle stores the scheduled training.
func (h ScheduleTrainingHandler) Handle(ctx context.Context, cmd ScheduleTraining) error {
	return h.repo.AddTraining(ctx, cmd)
}
