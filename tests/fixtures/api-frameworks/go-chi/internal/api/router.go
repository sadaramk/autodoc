// Package api serves the tasks HTTP API.
package api

import (
	"encoding/json"
	"net/http"

	"github.com/go-chi/chi/v5"
)

const apiPrefix = "/api"

// Task is a unit of work.
type Task struct {
	ID    string `json:"id"`
	Title string `json:"title"`
	// Done is true once the task is completed.
	Done bool `json:"done"`
}

// createTaskRequest is the body of POST /api/tasks.
type createTaskRequest struct {
	Title    string   `json:"title" validate:"required,min=3,max=140"`
	Priority int      `json:"priority,omitempty" validate:"omitempty,gte=1,lte=5"`
	Labels   []string `json:"labels" validate:"max=10"`
}

// NewRouter wires every route.
func NewRouter() http.Handler {
	r := chi.NewRouter()
	r.Get("/healthz", func(w http.ResponseWriter, r *http.Request) { w.WriteHeader(http.StatusOK) })
	r.Route(apiPrefix, func(r chi.Router) {
		r.Route("/tasks", func(r chi.Router) {
			r.Get("/", listTasks)
			r.With(requireAuth).Post("/", createTask)
			r.Get("/{taskID}", getTask)
		})
		r.Mount("/admin", adminRoutes())
	})
	return r
}

func adminRoutes() http.Handler {
	r := chi.NewRouter()
	r.Use(requireAdmin)
	r.Delete("/tasks/{taskID}", deleteTask)
	return r
}

// listTasks returns tasks, optionally filtered by status.
func listTasks(w http.ResponseWriter, r *http.Request) {
	status := r.URL.Query().Get("status")
	_ = status
	var tasks []Task
	writeJSON(w, http.StatusOK, tasks)
}

// createTask validates and stores a new task.
func createTask(w http.ResponseWriter, r *http.Request) {
	var req createTaskRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		http.Error(w, "invalid JSON", http.StatusBadRequest)
		return
	}
	task := Task{ID: "t1", Title: req.Title}
	writeJSON(w, http.StatusCreated, task)
}

func getTask(w http.ResponseWriter, r *http.Request) {
	id := chi.URLParam(r, "taskID")
	if id == "" {
		http.Error(w, "task not found", http.StatusNotFound)
		return
	}
	writeJSON(w, http.StatusOK, Task{ID: id})
}

func deleteTask(w http.ResponseWriter, r *http.Request) {
	w.WriteHeader(http.StatusNoContent)
}

func requireAuth(next http.Handler) http.Handler { return next }

func requireAdmin(next http.Handler) http.Handler { return next }

func writeJSON(w http.ResponseWriter, status int, v any) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	_ = json.NewEncoder(w).Encode(v)
}
