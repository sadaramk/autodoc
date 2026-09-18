// Package ports exposes the trainings service over HTTP.
package ports

import (
	"net/http"

	"github.com/acme/workouts/internal/trainings/app"
	"github.com/acme/workouts/internal/trainings/app/command"
	"github.com/acme/workouts/internal/trainings/app/query"
	"github.com/go-chi/chi/v5"
)

// HttpServer serves the trainings API.
type HttpServer struct {
	app app.Application
}

// NewHttpServer builds the server from the application.
func NewHttpServer(application app.Application) HttpServer {
	return HttpServer{app: application}
}

// Routes returns the trainings router.
func Routes(h HttpServer) http.Handler {
	r := chi.NewRouter()
	r.Get("/trainings", h.GetTrainings)
	r.Post("/trainings", h.CreateTraining)
	return r
}

// GetTrainings lists the caller's trainings.
func (h HttpServer) GetTrainings(w http.ResponseWriter, r *http.Request) {
	trainings, err := h.app.Queries.AllTrainings.Handle(r.Context())
	if err != nil {
		http.Error(w, err.Error(), 500)
		return
	}
	_ = trainings
}

// CreateTraining schedules a training.
func (h HttpServer) CreateTraining(w http.ResponseWriter, r *http.Request) {
	if err := h.app.Commands.ScheduleTraining.Handle(r.Context(), command.ScheduleTraining{}); err != nil {
		http.Error(w, err.Error(), 500)
	}
}
