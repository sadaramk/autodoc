// Package store keeps short-code to URL mappings.
package store

import "sync"

// Memory is a concurrency-safe in-memory store.
type Memory struct {
	mu   sync.RWMutex
	urls map[string]string
}

// NewMemory returns an empty store.
func NewMemory() *Memory {
	return &Memory{urls: make(map[string]string)}
}

// Put saves url under code.
func (m *Memory) Put(code, url string) {
	m.mu.Lock()
	defer m.mu.Unlock()
	m.urls[code] = url
}

// Get looks up the URL for code.
func (m *Memory) Get(code string) (string, bool) {
	m.mu.RLock()
	defer m.mu.RUnlock()
	url, ok := m.urls[code]
	return url, ok
}
