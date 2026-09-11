// Package cc is the Cascade SDK — HTTP + SSE client with snapshot fallback.
//
//	client, _ := cc.FromURL("cascade://localhost:7070/project/<id>?env=prod&token=xxx")
//	v, _ := client.Get("database.host")
//	client.Watch(func(event string) { ... })
//
// Secrets come back as errors (hidden) unless Reveal is enabled AND the
// token has admin permission. The snapshot stores exactly what the server
// returned.
package cascade

import (
	"bufio"
	"encoding/json"
	"fmt"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"strings"
	"time"
)

// Config is a raw config entry (write API responses).
type Config struct {
	ID          string  `json:"id"`
	Key         string  `json:"key"`
	Value       *string `json:"value"`
	Secret      bool    `json:"secret"`
	Group       *string `json:"group"`
	Description *string `json:"description"`
}

// ResolvedConfig is an effective value after env-chain resolution.
type ResolvedConfig struct {
	ID          string  `json:"id"`
	Key         string  `json:"key"`
	Value       *string `json:"value"`
	Secret      bool    `json:"secret"`
	Source      string  `json:"source"`
	Group       *string `json:"group"`
	Description *string `json:"description"`
}

type Client struct {
	Server       string
	ProjectID    string
	Token        string
	Env          string
	Reveal       bool
	SnapshotPath string
	HTTP         *http.Client
}

func NewClient(server, projectID, token string) *Client {
	return &Client{
		Server:       strings.TrimRight(server, "/"),
		ProjectID:    projectID,
		Token:        token,
		HTTP:         &http.Client{},
		SnapshotPath: defaultSnapshotPath(projectID),
	}
}

func defaultSnapshotPath(projectID string) string {
	home, err := os.UserHomeDir()
	if err != nil {
		return ""
	}
	return filepath.Join(home, ".cascade", "sdk", projectID+".json")
}

// FromURL parses a project link.
//
// Canonical:  cascade://<host>[:port]/project/<id>?env=<name>&token=<t>[&reveal=1]
// Shorthand:  cascade://project/<id>   (server defaults to http://localhost:7070)
// Legacy `cc://` links parse identically (scheme is ignored).
func FromURL(rawURL string) (*Client, error) {
	parsed, err := url.Parse(rawURL)
	if err != nil {
		return nil, err
	}
	token := parsed.Query().Get("token")
	env := parsed.Query().Get("env")
	reveal := false
	switch strings.ToLower(parsed.Query().Get("reveal")) {
	case "1", "true", "yes":
		reveal = true
	}

	var server, projectID string
	if parsed.Host == "project" || parsed.Host == "" {
		server = "http://localhost:7070"
		projectID = strings.Trim(parsed.Path, "/")
		if i := strings.LastIndex(projectID, "/"); i >= 0 {
			projectID = projectID[i+1:]
		}
	} else {
		host := parsed.Hostname()
		port := parsed.Port()
		if port == "" {
			port = "7070"
		}
		server = "http://" + host + ":" + port
		projectID = strings.Trim(strings.TrimPrefix(parsed.Path, "/project/"), "/")
	}
	if projectID == "" {
		return nil, fmt.Errorf("cannot parse project id from %q", rawURL)
	}

	c := NewClient(server, projectID, token)
	c.Env = env
	c.Reveal = reveal
	return c, nil
}

func (c *Client) headers() http.Header {
	h := http.Header{}
	h.Set("Content-Type", "application/json")
	if c.Token != "" {
		h.Set("Authorization", "Bearer "+c.Token)
	}
	return h
}

func (c *Client) resolvedURL() string {
	u := c.Server + "/api/projects/" + c.ProjectID + "/resolved"
	q := url.Values{}
	if c.Env != "" {
		q.Set("env", c.Env)
	}
	if c.Reveal {
		q.Set("reveal", "true")
	}
	if enc := q.Encode(); enc != "" {
		u += "?" + enc
	}
	return u
}

func (c *Client) fetchResolved() ([]ResolvedConfig, error) {
	req, err := http.NewRequest("GET", c.resolvedURL(), nil)
	if err != nil {
		return nil, err
	}
	req.Header = c.headers()
	resp, err := c.HTTP.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("HTTP %d", resp.StatusCode)
	}
	var out []ResolvedConfig
	if err := json.NewDecoder(resp.Body).Decode(&out); err != nil {
		return nil, err
	}
	c.writeSnapshot(out)
	return out, nil
}

func (c *Client) writeSnapshot(data []ResolvedConfig) {
	if c.SnapshotPath == "" {
		return
	}
	if err := os.MkdirAll(filepath.Dir(c.SnapshotPath), 0o755); err != nil {
		return
	}
	if b, err := json.Marshal(data); err == nil {
		_ = os.WriteFile(c.SnapshotPath, b, 0o644)
	}
}

func (c *Client) readSnapshot() []ResolvedConfig {
	if c.SnapshotPath == "" {
		return nil
	}
	b, err := os.ReadFile(c.SnapshotPath)
	if err != nil {
		return nil
	}
	var out []ResolvedConfig
	if json.Unmarshal(b, &out) != nil {
		return nil
	}
	return out
}

// data returns resolved configs; falls back to the last snapshot when the
// server is unreachable (startup must not be blocked).
func (c *Client) data() []ResolvedConfig {
	if d, err := c.fetchResolved(); err == nil {
		return d
	}
	return c.readSnapshot()
}

// Get returns the effective value for this project/env.
func (c *Client) Get(key string) (string, error) {
	for _, cfg := range c.data() {
		if cfg.Key == key {
			if cfg.Value != nil {
				return *cfg.Value, nil
			}
			return "", fmt.Errorf("secret %q is hidden (use reveal with an admin token)", key)
		}
	}
	return "", fmt.Errorf("key not found: %s", key)
}

// ListConfigs returns resolved configs, optionally filtered by group.
func (c *Client) ListConfigs(group string) ([]ResolvedConfig, error) {
	all := c.data()
	if group == "" {
		return all, nil
	}
	out := make([]ResolvedConfig, 0, len(all))
	for _, cfg := range all {
		if cfg.Group != nil && *cfg.Group == group {
			out = append(out, cfg)
		}
	}
	return out, nil
}

// Refresh forces a fetch (or snapshot fallback).
func (c *Client) Refresh() []ResolvedConfig {
	return c.data()
}

// Set creates or updates a raw config entry.
func (c *Client) Set(key, value string, secret bool) (*Config, error) {
	body := map[string]interface{}{"key": key, "value": value, "secret": secret}
	jsonBody, _ := json.Marshal(body)
	req, err := http.NewRequest("POST", c.Server+"/api/configs", strings.NewReader(string(jsonBody)))
	if err != nil {
		return nil, err
	}
	req.Header = c.headers()
	resp, err := c.HTTP.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("HTTP %d", resp.StatusCode)
	}
	var config Config
	if err := json.NewDecoder(resp.Body).Decode(&config); err != nil {
		return nil, err
	}
	return &config, nil
}

// Watch streams SSE change events in a goroutine with auto-reconnect.
// The callback receives the raw event, e.g. "config_updated:<id>".
func (c *Client) Watch(callback func(event string)) {
	go func() {
		for {
			req, err := http.NewRequest("GET", c.Server+"/api/sse/configs", nil)
			if err == nil {
				req.Header = c.headers()
				resp, err := c.HTTP.Do(req)
				if err == nil && resp.StatusCode == http.StatusOK {
					scanner := bufio.NewScanner(resp.Body)
					for scanner.Scan() {
						line := scanner.Text()
						if strings.HasPrefix(line, "data:") {
							callback(strings.TrimSpace(strings.TrimPrefix(line, "data:")))
						}
					}
					resp.Body.Close()
				} else if resp != nil {
					resp.Body.Close()
				}
			}
			time.Sleep(3 * time.Second)
		}
	}()
}

func (c *Client) exportPairs() map[string]string {
	out := map[string]string{}
	for _, cfg := range c.data() {
		if cfg.Value != nil {
			out[cfg.Key] = *cfg.Value
		} else {
			out[cfg.Key] = "${" + strings.ToUpper(strings.ReplaceAll(cfg.Key, ".", "_")) + "}"
		}
	}
	return out
}

// ExportFile writes yaml or json of the resolved config.
func (c *Client) ExportFile(path, format string) error {
	data := c.exportPairs()
	var content string
	switch format {
	case "json":
		b, _ := json.MarshalIndent(data, "", "  ")
		content = string(b)
	case "yaml":
		var sb strings.Builder
		for k, v := range data {
			sb.WriteString(fmt.Sprintf("%s: \"%s\"\n", k, strings.ReplaceAll(v, "\"", "\\\"")))
		}
		content = sb.String()
	default:
		return fmt.Errorf("unsupported format: %s", format)
	}
	return os.WriteFile(path, []byte(content), 0o644)
}

// ExportEnv writes a dotenv file (secrets as ${PLACEHOLDER}).
func (c *Client) ExportEnv(path string) error {
	lines := []string{"# generated by cascade"}
	for key, value := range c.exportPairs() {
		envName := strings.ToUpper(strings.ReplaceAll(key, ".", "_"))
		escaped := strings.NewReplacer("\\", "\\\\", "\"", "\\\"", "\n", "\\n").Replace(value)
		lines = append(lines, fmt.Sprintf("%s=\"%s\"", envName, escaped))
	}
	return os.WriteFile(path, []byte(strings.Join(lines, "\n")+"\n"), 0o644)
}
