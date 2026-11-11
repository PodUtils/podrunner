package main

import (
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"strings"
	"sync"
	"time"
)

// The trust inputs of one run: each value is one of the words of
// job-offer.v1, and "unknown" when the lookup failed.
type trustInputs struct {
	event, headIsFork, actorClass, visibility string
}

// The events that the tier rule names; another is "other".
var knownEvents = map[string]bool{
	"push": true, "pull_request": true, "pull_request_target": true, "workflow_dispatch": true, "schedule": true,
	"merge_group": true, "workflow_run": true, "issue_comment": true, "release": true, "repository_dispatch": true,
}

func eventWord(e string) string {
	switch {
	case e == "":
		return "unknown"
	case knownEvents[e]:
		return e
	default:
		return "other"
	}
}

// trust reads the run object (repository "Actions: read") and the
// membership of the actor (organization "Members: read"), with the token
// of the installation. The result is kept for each run id.
type trust struct {
	mu           sync.Mutex
	api          string
	installation int64
	signer       *signer
	token        string
	expires      time.Time
	cache        map[int64]trustInputs
	http         http.Client
}

func (t *trust) installationToken() (string, error) {
	if t.token != "" && time.Until(t.expires) > 5*time.Minute {
		return t.token, nil
	}
	jwt, err := t.signer.token(t.installation)
	if err != nil {
		return "", err
	}
	req, _ := http.NewRequest(http.MethodPost, fmt.Sprintf("%s/app/installations/%d/access_tokens", t.api, t.installation), nil)
	req.Header.Set("Authorization", "Bearer "+jwt)
	req.Header.Set("Accept", "application/vnd.github+json")
	resp, err := t.http.Do(req)
	if err != nil {
		return "", fmt.Errorf("the installation token: no answer")
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusCreated {
		return "", fmt.Errorf("the installation token: HTTP %d", resp.StatusCode)
	}
	var v struct {
		Token     string    `json:"token"`
		ExpiresAt time.Time `json:"expires_at"`
	}
	if err := json.NewDecoder(io.LimitReader(resp.Body, 1<<16)).Decode(&v); err != nil || v.Token == "" {
		return "", fmt.Errorf("the installation token: no token")
	}
	t.token, t.expires = v.Token, v.ExpiresAt
	return t.token, nil
}

func (t *trust) get(path string) (int, map[string]any) {
	token, err := t.installationToken()
	if err != nil {
		return 0, nil
	}
	req, _ := http.NewRequest(http.MethodGet, t.api+path, nil)
	req.Header.Set("Authorization", "Bearer "+token)
	req.Header.Set("Accept", "application/vnd.github+json")
	resp, err := t.http.Do(req)
	if err != nil {
		return 0, nil
	}
	defer resp.Body.Close()
	var v map[string]any
	_ = json.NewDecoder(io.LimitReader(resp.Body, 1<<20)).Decode(&v)
	return resp.StatusCode, v
}

// lookup gives the trust inputs of a run; ok is false when a part is
// unknown. A failed lookup is not kept: the next job tries again.
func (t *trust) lookup(owner, repo string, run int64, messageEvent string) (trustInputs, bool) {
	t.mu.Lock()
	defer t.mu.Unlock()
	if v, ok := t.cache[run]; ok {
		return v, true
	}
	unknown := trustInputs{event: eventWord(messageEvent), headIsFork: "unknown", actorClass: "unknown", visibility: "unknown"}
	status, r := t.get(fmt.Sprintf("/repos/%s/%s/actions/runs/%d", owner, repo, run))
	if status != http.StatusOK || r == nil {
		return unknown, false
	}
	out := unknown
	if e, ok := r["event"].(string); ok {
		out.event = eventWord(e)
	}
	if h, ok := r["head_repository"].(map[string]any); ok {
		if f, ok := h["fork"].(bool); ok {
			out.headIsFork = map[bool]string{true: "yes", false: "no"}[f]
		}
	}
	if rp, ok := r["repository"].(map[string]any); ok {
		switch v, _ := rp["visibility"].(string); v {
		case "private", "internal", "public":
			out.visibility = v
		}
	}
	actor := ""
	if a, ok := r["actor"].(map[string]any); ok {
		actor, _ = a["login"].(string)
	}
	if actor != "" && !strings.ContainsAny(actor, "/?#") {
		switch code, _ := t.get(fmt.Sprintf("/orgs/%s/members/%s", owner, actor)); code {
		case http.StatusNoContent:
			out.actorClass = "member"
		case http.StatusNotFound:
			out.actorClass = "outside"
		}
	}
	whole := out.headIsFork != "unknown" && out.actorClass != "unknown" && out.visibility != "unknown"
	if whole {
		t.cache[run] = out
	}
	return out, whole
}
