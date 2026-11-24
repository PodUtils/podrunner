package main

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net"
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
	// The last run object of each run id and its ETag: a read of a run
	// that did not change is a 304, which GitHub does not count.
	runs map[int64]seenRun
	http http.Client
	// The calls of GitHub in the hour that runs, and its start: a lookup
	// past the budget is not made, and its inputs are "unknown" (deny).
	budget    int
	used      int
	hourStart time.Time
	Throttled int
}

// runKey: a re-run keeps the run id and the actor; its triggering actor
// can be another account (T-504 review).
type runKey struct{ run, attempt int64 }

type seenRun struct {
	etag string
	obj  map[string]any
}

// The caches hold at most this many runs: a flood of runs must not grow
// the memory of the sidecar.
const cacheMax = 4096

// A call of GitHub must not hold the wire: one call, and the lookups of
// one `next`, end well inside the reply limit of the mint (15 s,
// REPLY_LIMIT of crates/podrunner-mint/src/sidecar.rs).
var (
	lookupTimeout = 5 * time.Second
	nextBudget    = 10 * time.Second
)

// errPending: a call did not end in its time. The job stays queued: it
// is not refused and not lost.
var errPending = errors.New("the lookup did not end in its time")

// errThrottled: the budget of the hour is spent.
var errThrottled = errors.New("the trust budget of the hour is spent")

// batch holds the lookups of one `next`: each run is read once, and each
// login is looked up once, for all its jobs.
type batch struct {
	ctx     context.Context
	runs    map[int64]batchRun
	members map[string]string
}

type batchRun struct {
	in    trustInputs
	whole bool
}

func newBatch(ctx context.Context) *batch {
	return &batch{ctx: ctx, runs: map[int64]batchRun{}, members: map[string]string{}}
}

func newTrust(o options, signer *signer) *trust {
	api := o.apiURL
	if api == "" {
		api = apiURLOf(o.configURL)
	}
	// A slash at the end would double the slash of each path.
	return &trust{api: strings.TrimRight(api, "/"), installation: o.installationID, signer: signer, pat: o.pat,
		budget: o.trustBudget, http: http.Client{Timeout: lookupTimeout}}
}

// spend counts one call of GitHub in the budget of the hour. With gate,
// a call past the budget is refused.
func (t *trust) spend(gate bool) bool {
	if time.Since(t.hourStart) >= time.Hour {
		t.hourStart, t.used = time.Now(), 0
	}
	if gate && t.budget > 0 && t.used >= t.budget {
		t.Throttled++
		return false
	}
	t.used++
	return true
}

// failed gives errPending for a call that did not end in its time.
func failed(ctx context.Context, err error) error {
	var ne net.Error
	if ctx.Err() != nil || errors.Is(err, context.DeadlineExceeded) || (errors.As(err, &ne) && ne.Timeout()) {
		return errPending
	}
	return err
}

func (t *trust) installationToken(ctx context.Context) (string, error) {
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
	req, _ := http.NewRequestWithContext(ctx, http.MethodPost,
		fmt.Sprintf("%s/app/installations/%d/access_tokens", t.api, t.installation), nil)
	req.Header.Set("Authorization", "Bearer "+jwt)
	req.Header.Set("Accept", "application/vnd.github+json")
	t.spend(false)
	resp, err := t.http.Do(req)
	if err != nil {
		if failed(ctx, err) == errPending {
			return "", errPending
		}
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

// get makes one GET of the REST API in the budget. With etag, GitHub
// answers 304 when the object did not change: a 304 is not counted.
func (t *trust) get(ctx context.Context, path, etag string) (int, map[string]any, string, error) {
	token, err := t.installationToken(ctx)
	if err != nil {
		return 0, nil, "", err
	}
	if !t.spend(true) {
		return 0, nil, "", errThrottled
	}
	req, _ := http.NewRequestWithContext(ctx, http.MethodGet, t.api+path, nil)
	req.Header.Set("Authorization", "Bearer "+token)
	req.Header.Set("Accept", "application/vnd.github+json")
	if etag != "" {
		req.Header.Set("If-None-Match", etag)
	}
	resp, err := t.http.Do(req)
	if err != nil {
		return 0, nil, "", failed(ctx, err)
	}
	defer resp.Body.Close()
	if resp.StatusCode == http.StatusNotModified {
		t.used--
		return resp.StatusCode, nil, etag, nil
	}
	var v map[string]any
	if err := json.NewDecoder(io.LimitReader(resp.Body, 1<<20)).Decode(&v); err != nil && failed(ctx, err) == errPending {
		return 0, nil, "", errPending
	}
	return resp.StatusCode, v, resp.Header.Get("ETag"), nil
}

// lookup gives the trust inputs of a run in a batch of its own; ok is
// false when a part is unknown.
func (t *trust) lookup(owner, repo string, run int64, messageEvent string) (trustInputs, bool) {
	in, whole, _ := t.lookupIn(newBatch(context.Background()), owner, repo, run, messageEvent)
	return in, whole
}

// lookupIn gives the trust inputs of a run, once for each run of a batch.
// pending is true when a call did not end in the time of the batch: the
// job stays queued. A failed lookup is not kept: a later batch tries
// again.
func (t *trust) lookupIn(b *batch, owner, repo string, run int64, messageEvent string) (in trustInputs, whole, pending bool) {
	if r, ok := b.runs[run]; ok {
		return r.in, r.whole, false
	}
	if b.ctx.Err() != nil {
		return in, false, true
	}
	t.mu.Lock()
	defer t.mu.Unlock()
	in, whole, err := t.read(b, owner, repo, run, messageEvent)
	if err == errPending {
		return in, false, true
	}
	b.runs[run] = batchRun{in, whole}
	return in, whole, false
}

// read reads the run object: only it names the attempt. It is a
// conditional read when the run was read before.
func (t *trust) read(b *batch, owner, repo string, run int64, messageEvent string) (trustInputs, bool, error) {
	unknown := trustInputs{event: eventWord(messageEvent), headIsFork: "unknown", actorClass: "unknown", visibility: "unknown"}
	seen := t.runs[run]
	status, r, etag, err := t.get(b.ctx, fmt.Sprintf("/repos/%s/%s/actions/runs/%d", url.PathEscape(owner), url.PathEscape(repo), run), seen.etag)
	switch {
	case err != nil:
		return unknown, false, err
	case status == http.StatusNotModified && seen.obj != nil:
		r = seen.obj
	case status != http.StatusOK || r == nil:
		return unknown, false, nil
	case etag != "":
		if t.runs == nil || len(t.runs) >= cacheMax {
			t.runs = map[int64]seenRun{}
		}
		t.runs[run] = seenRun{etag, r}
	}
	attempt, counted := r["run_attempt"].(float64)
	key := runKey{run, int64(attempt)}
	if v, ok := t.cache[key]; ok && counted {
		return v, true, nil
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
	out.actorClass, err = t.classOf(b, owner, r["actor"])
	if err != nil {
		return unknown, false, err
	}
	switch v, ok := r["triggering_actor"]; {
	case ok && v != nil:
		c, err := t.classOf(b, owner, v)
		if err != nil {
			return unknown, false, err
		}
		out.actorClass = stricter(out.actorClass, c)
	case counted && attempt > 1:
		// A re-run with no triggering actor names no one who started it.
		out.actorClass = "unknown"
	}
	whole := out.headIsFork != "unknown" && out.actorClass != "unknown" && out.visibility != "unknown"
	if whole && counted {
		if t.cache == nil || len(t.cache) >= cacheMax {
			t.cache = map[runKey]trustInputs{}
		}
		t.cache[key] = out
	}
	return out, whole, nil
}

// classOf gives the class of one actor object of a run. The batch keeps
// the class of each login: one lookup for each login.
func (t *trust) classOf(b *batch, owner string, v any) (string, error) {
	a, _ := v.(map[string]any)
	login, _ := a["login"].(string)
	switch {
	case login == "":
		return "unknown", nil
	case t.personal:
		// A personal account has no members (T-504): only the account
		// itself is a member. A collaborator is outside: deny by default.
		if asciiFold(owner, t.account) && t.isAccount(a, login) {
			return "member", nil
		}
		return "outside", nil
	case strings.ContainsAny(login, "/?#"):
		return "unknown", nil
	}
	key := owner + "/" + login
	if c, ok := b.members[key]; ok {
		return c, nil
	}
	c := "unknown"
	code, _, _, err := t.get(b.ctx, fmt.Sprintf("/orgs/%s/members/%s", url.PathEscape(owner), url.PathEscape(login)), "")
	switch {
	case err == errPending:
		return "", err
	case code == http.StatusNoContent:
		c = "member"
	case code == http.StatusNotFound:
		c = "outside"
	}
	b.members[key] = c
	return c, nil
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
