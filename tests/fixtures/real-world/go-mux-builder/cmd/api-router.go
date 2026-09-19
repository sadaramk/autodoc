// Package cmd registers the object API.
package cmd

import (
	"net/http"

	"github.com/minio/mux"
)

// objectAPIHandlers serves the object API.
type objectAPIHandlers struct{}

// GetObjectHandler returns an object's bytes.
func (api objectAPIHandlers) GetObjectHandler(w http.ResponseWriter, r *http.Request) {}

// HeadObjectHandler returns an object's metadata.
func (api objectAPIHandlers) HeadObjectHandler(w http.ResponseWriter, r *http.Request) {}

// GetObjectTaggingHandler returns the tags set on an object.
func (api objectAPIHandlers) GetObjectTaggingHandler(w http.ResponseWriter, r *http.Request) {}

// PutObjectHandler stores an object.
func (api objectAPIHandlers) PutObjectHandler(w http.ResponseWriter, r *http.Request) {}

// ListBucketsHandler lists every bucket.
func (api objectAPIHandlers) ListBucketsHandler(w http.ResponseWriter, r *http.Request) {}

// apiMiddleware wraps a handler with tracing common to the object API.
func apiMiddleware(f http.HandlerFunc) http.HandlerFunc {
	return f
}

// registerAPIRouter wires the object API onto the router.
func registerAPIRouter(router *mux.Router, api objectAPIHandlers) {
	apiRouter := router.PathPrefix("/").Subrouter()
	bucketRouter := apiRouter.PathPrefix("/{bucket}").Subrouter()

	// Two routes share a method and path; the query tells them apart.
	bucketRouter.Methods(http.MethodGet).Path("/{object:.+}").
		HandlerFunc(apiMiddleware(api.GetObjectTaggingHandler)).
		Queries("tagging", "")
	bucketRouter.Methods(http.MethodGet).Path("/{object:.+}").
		HandlerFunc(apiMiddleware(api.GetObjectHandler))
	bucketRouter.Methods(http.MethodHead).Path("/{object:.+}").
		HandlerFunc(apiMiddleware(api.HeadObjectHandler))
	bucketRouter.Methods(http.MethodPut).Path("/{object:.+}").
		HandlerFunc(apiMiddleware(api.PutObjectHandler))
	apiRouter.Methods(http.MethodGet).Path("/").
		HandlerFunc(apiMiddleware(api.ListBucketsHandler))
}
