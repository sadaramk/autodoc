package ports

import (
	"encoding/json"
	"net/http"
)

// HourUpdate is what the HTTP endpoint accepts.
type HourUpdate struct {
	Hours []string `json:"hours"`
}

type HttpServer struct{}

// MakeHourAvailable marks hours as available for training.
func (h HttpServer) MakeHourAvailable(w http.ResponseWriter, r *http.Request) {
	update := HourUpdate{}
	if err := json.NewDecoder(r.Body).Decode(&update); err != nil {
		w.WriteHeader(http.StatusBadRequest)
		return
	}
	w.WriteHeader(http.StatusNoContent)
}
