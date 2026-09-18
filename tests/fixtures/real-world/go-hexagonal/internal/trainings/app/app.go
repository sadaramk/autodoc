// Package app wires the command and query sides.
package app

import (
	"github.com/acme/workouts/internal/trainings/app/command"
	"github.com/acme/workouts/internal/trainings/app/query"
)

// Application is the CQRS entry point.
type Application struct {
	Commands Commands
	Queries  Queries
}

// Commands are the write side handlers.
type Commands struct {
	ScheduleTraining command.ScheduleTrainingHandler
}

// Queries are the read side handlers.
type Queries struct {
	AllTrainings query.AllTrainingsHandler
}
