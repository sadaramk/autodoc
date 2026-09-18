// Package hash derives short codes from URLs.
package hash

import (
	"crypto/sha256"
	"encoding/base64"
)

// Code returns a stable 7-character code for url.
func Code(url string) string {
	sum := sha256.Sum256([]byte(url))
	return base64.RawURLEncoding.EncodeToString(sum[:])[:7]
}
