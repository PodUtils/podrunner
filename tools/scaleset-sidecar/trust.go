package main

import (
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
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
	// pat: the token of descriptor 3 is the token of the lookups (T-174).
	pat bool
	// personal: the scale set is of a repository of the personal account
	// `account` (T-504). It has no members: see lookup.
	personal bool
	account  string
	signer   *signer
	token    string
	expires  time.Time
	cache    map[int64]trustInputs
	http     http.Client
	// The lookups of the hour that runs, and its start: a lookup past the
	// budget is not made, and its inputs are "unknown" (throttled).
	budget    int
	used      int
	hourStart time.Time
	Throttled int
}

func (t *trust) installationToken() (string, error) {
	if t.pat {
		return t.signer.pat()
	}
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
	if time.Since(t.hourStart) >= time.Hour {
		t.hourStart, t.used = time.Now(), 0
	}
	if t.budget > 0 && t.used >= t.budget {
		t.Throttled++
		return unknown, false
	}
	t.used++
	status, r := t.get(fmt.Sprintf("/repos/%s/%s/actions/runs/%d", owner, repo, run))
	if status != http.StatusOK || r == nil {
		return unknown, false
	}
	out := unknown
	if e, ok := r["event"].(string); ok {
		out.event = eventWord(e)
	}
	out.headIsFork = headIsFork(r)
	if rp, ok := r["repository"].(map[string]any); ok {
		out.visibility = visibilityOf(rp)
	}
	actor := ""
	if a, ok := r["actor"].(map[string]any); ok {
		actor, _ = a["login"].(string)
	}
	switch {
	case actor == "":
	case t.personal:
		// A personal account has no members (T-504): only the account
		// itself is a member. A collaborator is outside: deny by default.
		out.actorClass = "outside"
		if strings.EqualFold(actor, owner) && strings.EqualFold(owner, t.account) {
			out.actorClass = "member"
		}
	case !strings.ContainsAny(actor, "/?#"):
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

// The refusal of a personal account with the opt-in off (call C3, T-504).
const personalOff = "repo-scope-refused: %s is not an organization (a personal account needs Administration: write; " +
	"to serve it, set allow_personal_account = true in its [[source]])"

// scope checks the config URL before the sidecar asks GitHub for a scale
// set (call C3): the URL of an organization; with personal (T-504), also
// the URL of a repository of a personal account. A repository of an
// organization is refused: its scale set needs Administration: write.
func (t *trust) scope(configURL string, personal bool) error {
	parts := []string{""}
	if u, err := url.Parse(strings.Trim(configURL, "/")); err == nil {
		parts = strings.Split(strings.Trim(u.Path, "/"), "/")
	}
	if parts[0] == "" || len(parts) > 2 || strings.EqualFold(parts[0], "enterprises") {
		return fmt.Errorf("repo-scope-refused: %s is not the URL of an organization or of a repository", configURL)
	}
	owner := parts[0]
	org, err := t.organization(owner)
	switch {
	case err != nil:
		return err
	case org && len(parts) == 2:
		return fmt.Errorf("repo-scope-refused: %s/%s is a repository of an organization: give config_url the URL "+
			"of the organization (a repository scale set needs Administration: write)", owner, parts[1])
	case org:
		return nil
	case !personal:
		return fmt.Errorf(personalOff, owner)
	case len(parts) == 1:
		return fmt.Errorf("repo-scope-refused: %s is a personal account: give config_url the URL of one of its "+
			"repositories (https://github.com/%s/REPO)", owner, owner)
	}
	t.personal, t.account = true, owner
	return nil
}

// organization tells whether the owner is an organization: GitHub answers
// 404 for a personal account.
func (t *trust) organization(owner string) (bool, error) {
	t.mu.Lock()
	defer t.mu.Unlock()
	switch code, v := t.get("/orgs/" + url.PathEscape(owner)); {
	case code == http.StatusOK && v != nil:
		return true, nil
	case code == http.StatusNotFound:
		return false, nil
	default:
		return false, fmt.Errorf("the organization %s: HTTP %d", owner, code)
	}
}

// visibilityOf reads the visibility of a repository. The repository of a
// run is a minimal one: `visibility` is null, and only `private` is given
// (T-167). The tier rule treats private and internal the same, and an
// internal repository is private, so `private` alone is enough.
func visibilityOf(rp map[string]any) string {
	switch v, _ := rp["visibility"].(string); v {
	case "private", "internal", "public":
		return v
	}
	switch p, ok := rp["private"].(bool); {
	case ok && p:
		return "private"
	case ok:
		return "public"
	}
	return "unknown"
}

// headIsFork tells whether the head of a run is another repository than
// its base. Not `head_repository.fork`: a pull request can come from any
// repository of the fork network, also from its root (fork=false), and a
// push to a fork has its own repository as the head (fork=true).
func headIsFork(r map[string]any) string {
	head, _ := r["head_repository"].(map[string]any)
	base, _ := r["repository"].(map[string]any)
	if head == nil || base == nil {
		return "unknown"
	}
	fork := map[bool]string{true: "yes", false: "no"}
	hid, hok := head["id"].(float64)
	bid, bok := base["id"].(float64)
	if hok && bok {
		return fork[hid != bid]
	}
	// GitHub names do not care about case.
	hn, _ := head["full_name"].(string)
	bn, _ := base["full_name"].(string)
	if hn != "" && bn != "" {
		return fork[!strings.EqualFold(hn, bn)]
	}
	return "unknown"
}
