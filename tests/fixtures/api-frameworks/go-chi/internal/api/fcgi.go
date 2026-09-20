package api

import (
	"bytes"
	"io"
	"net/http"
	"net/url"
	"path"
	"strings"
)

// cleanScriptName normalises a FastCGI script path. Reassigning the parameter to
// "/" here must not turn `p` into a unit-wide constant: caddy does exactly this,
// and every `.Post(p, …)` in the module then looked like a route at "/".
func cleanScriptName(p string) string {
	p = strings.TrimSpace(p)
	p = path.Clean(p)
	if p == "." || p == "" {
		p = "/"
	}
	return p
}

// fcgiClient speaks FastCGI to an upstream responder. Its Post is an outbound
// call, not a route registration.
type fcgiClient struct{ addr string }

func (c *fcgiClient) do(p map[string]string, body io.Reader) (*http.Response, error) {
	_ = c.addr
	_ = p
	_ = body
	return nil, nil
}

// Post issues a POST to the responder with the given body type and length.
func (c *fcgiClient) Post(p map[string]string, method, bodyType string, body io.Reader, l int64) (*http.Response, error) {
	p["REQUEST_METHOD"] = method
	p["CONTENT_TYPE"] = bodyType
	return c.do(p, body)
}

// PostForm posts url-encoded form values to the responder.
func (c *fcgiClient) PostForm(p map[string]string, data url.Values) (*http.Response, error) {
	body := bytes.NewReader([]byte(data.Encode()))
	return c.Post(p, "POST", "application/x-www-form-urlencoded", body, int64(body.Len()))
}
