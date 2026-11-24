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
// membership of its actors (organization "Members: read"), with the token
// of the installation. The result is kept for each attempt of a run.
type trust struct {
	mu           sync.Mutex
	api          string
	installation int64
	// pat: the token of descriptor 3 is the token of the lookups (T-174).
	pat bool
	// personal: the scale set is of a repository of the personal account
	// `account` (T-504). It has no members: see lookup.
	personal  bool
	account   string
	accountID int64
	signer    *signer
	token     string
	expires   time.Time
	cache     map[runKey]trustInputs
	http      http.Client
	// The lookups of the hour that runs, and its start: a lookup past the
	// budget is not made, and its inputs are "unknown" (throttled).
	budget    int
	used      int
	hourStart time.Time
	Throttled int
}

// runKey: a re-run keeps the run id and the actor; its triggering actor
// can be another account (T-504 review).
type runKey struct{ run, attempt int64 }

// A lookup that GitHub does not answer must not hold the wire.
const lookupTimeout = 30 * time.Second

func newTrust(o options, signer *signer) *trust {
	api := o.apiURL
	if api == "" {
		api = apiURLOf(o.configURL)
	}
	return &trust{api: api, installation: o.installationID, signer: signer, pat: o.pat, budget: o.trustBudget,
		http: http.Client{Timeout: lookupTimeout}}
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
// unknown. A failed lookup is not kept: the next job tries again. The run
// object is read for each job: only it names the attempt.
func (t *trust) lookup(owner, repo string, run int64, messageEvent string) (trustInputs, bool) {
	t.mu.Lock()
	defer t.mu.Unlock()
	unknown := trustInputs{event: eventWord(messageEvent), headIsFork: "unknown", actorClass: "unknown", visibility: "unknown"}
	if time.Since(t.hourStart) >= time.Hour {
		t.hourStart, t.used = time.Now(), 0
	}
	if t.budget > 0 && t.used >= t.budget {
		t.Throttled++
		return unknown, false
	}
	t.used++
	status, r := t.get(fmt.Sprintf("/repos/%s/%s/actions/runs/%d", url.PathEscape(owner), url.PathEscape(repo), run))
	if status != http.StatusOK || r == nil {
		return unknown, false
	}
	attempt, counted := r["run_attempt"].(float64)
	key := runKey{run, int64(attempt)}
	if v, ok := t.cache[key]; ok && counted {
		return v, true
	}
	out := unknown
	if e, ok := r["event"].(string); ok {
		out.event = eventWord(e)
	}
	out.headIsFork = headIsFork(r)
	if rp, ok := r["repository"].(map[string]any); ok {
		out.visibility = visibilityOf(rp)
	}
	// The actor triggered the first attempt; the triggering actor started
	// this one (a re-run). Each must have the class: the stricter wins.
	classes := map[string]string{}
	out.actorClass = t.classOf(owner, r["actor"], classes)
	if v, ok := r["triggering_actor"]; ok && v != nil {
		out.actorClass = stricter(out.actorClass, t.classOf(owner, v, classes))
	}
	whole := out.headIsFork != "unknown" && out.actorClass != "unknown" && out.visibility != "unknown"
	if whole && counted {
		if t.cache == nil {
			t.cache = map[runKey]trustInputs{}
		}
		t.cache[key] = out
	}
	return out, whole
}

// classOf gives the class of one actor object of a run. done keeps the
// class of each login of this run: one lookup for each login.
func (t *trust) classOf(owner string, v any, done map[string]string) string {
	a, _ := v.(map[string]any)
	login, _ := a["login"].(string)
	switch {
	case login == "":
		return "unknown"
	case t.personal:
		// A personal account has no members (T-504): only the account
		// itself is a member. A collaborator is outside: deny by default.
		if asciiFold(owner, t.account) && t.isAccount(a, login) {
			return "member"
		}
		return "outside"
	case strings.ContainsAny(login, "/?#"):
		return "unknown"
	}
	if c, ok := done[login]; ok {
		return c
	}
	c := "unknown"
	switch code, _ := t.get(fmt.Sprintf("/orgs/%s/members/%s", url.PathEscape(owner), url.PathEscape(login))); code {
	case http.StatusNoContent:
		c = "member"
	case http.StatusNotFound:
		c = "outside"
	}
	done[login] = c
	return c
}

// isAccount: the id names the account when the run and the scope give
// one; a login can change hands after a rename.
func (t *trust) isAccount(a map[string]any, login string) bool {
	if id, ok := a["id"].(float64); ok && t.accountID != 0 {
		return int64(id) == t.accountID
	}
	return asciiFold(login, t.account)
}

// stricter gives the class that trusts less: unknown, then outside.
func stricter(a, b string) string {
	for _, c := range []string{"unknown", "outside"} {
		if a == c || b == c {
			return c
		}
	}
	return "member"
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
		return fork[!asciiFold(hn, bn)]
	}
	return "unknown"
}
