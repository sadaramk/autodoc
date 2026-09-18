// Package handlers implements the shortener HTTP API.
package handlers

import (
	"fmt"
	"net/http"

	"example.com/urlshort/internal/hash"
	"example.com/urlshort/internal/store"
)

// Handlers serves shorten and redirect requests.
type Handlers struct {
	store *store.Memory
}

// New builds handlers over a store.
func New(s *store.Memory) *Handlers {
	return &Handlers{store: s}
}

// Routes returns the HTTP mux.
func (h *Handlers) Routes() http.Handler {
	mux := http.NewServeMux()
	mux.HandleFunc("POST /shorten", h.shorten)
	mux.HandleFunc("GET /{code}", h.redirect)
	return mux
}

// shorten stores the url form value and returns its short code.
func (h *Handlers) shorten(w http.ResponseWriter, r *http.Request) {
	url := r.FormValue("url")
	code := hash.Code(url)
	h.store.Put(code, url)
	fmt.Fprintln(w, code)
}

// redirect sends the client to the stored URL.
func (h *Handlers) redirect(w http.ResponseWriter, r *http.Request) {
	url, ok := h.store.Get(r.PathValue("code"))
	if !ok {
		http.NotFound(w, r)
		return
	}
	http.Redirect(w, r, url, http.StatusFound)
}
