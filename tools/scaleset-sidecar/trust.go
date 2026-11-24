package main

import (
	"context"
	"errors"
	"fmt"
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
	// The inputs of each attempt of a run, the last run object of each run
	// id with its ETag, and the class of each login: each with its time.
	cache   map[runKey]keptInputs
	runs    map[int64]seenRun
	members map[string]keptClass
	http    http.Client
	// The calls of GitHub in the hour that runs, and its start: a lookup
	// past the budget is not made, and its job waits (calls.go).
	budget    int
	used      int
	hourStart time.Time
	spentSaid bool
	Throttled int
}

// runKey: a re-run keeps the run id and the actor; its triggering actor
// can be another account (T-504 review).
type runKey struct{ run, attempt int64 }

// seenRun: a run object, its ETag, and the time its read began.
type seenRun struct {
	etag string
	obj  map[string]any
	at   time.Time
}

type keptInputs struct {
	in trustInputs
	at time.Time
}

type keptClass struct {
	class string
	at    time.Time
}

// The caches hold at most this many runs and logins: a flood of runs must
// not grow the memory of the sidecar.
const cacheMax = 4096

// The time that a result stays usable. The inputs of an attempt hold its
// membership classes: a member who leaves the organization must not stay
// a member. A run object and the class of a login are the parts of a
// lookup that a `next` gives the next one: short.
var (
	cacheTTL   = 10 * time.Minute
	partialTTL = 2 * time.Minute
)

// lookupState: how the lookup of a run ended in one `next`.
type lookupState int

const (
	// final: GitHub answered; the inputs can hold unknown.
	final lookupState = iota
	// retry: a call gave no final answer: an attempt.
	retry
	// throttled: the budget of the hour is spent; no call was made.
	throttled
	// notReached: the time of the `next` ended first.
	notReached
)

// batch holds the lookups of one `next`: each run is read once, and each
// login is looked up once, for all its jobs.
type batch struct {
	ctx     context.Context
	runs    map[int64]batchRun
	members map[string]string
}

// batchRun: the result of a run in a batch. at: the time that the read
// of its run object began. A final result serves only the jobs that came
// before at: a later job can be of a re-run.
type batchRun struct {
	in    trustInputs
	whole bool
	st    lookupState
	why   string
	at    time.Time
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

// lookup gives the trust inputs of a run in a batch of its own; ok is
// false when a part is unknown or the lookup did not end.
func (t *trust) lookup(owner, repo string, run int64, messageEvent string) (trustInputs, bool) {
	in, whole, _, _ := t.lookupIn(newBatch(context.Background()), owner, repo, run, messageEvent, time.Now())
	return in, whole
}

// lookupIn gives the trust inputs of a run for a job that came at
// `since`. A batch reads each run once for the jobs that came before the
// read; a job that came after it gets a read of its own. A result with no
// final answer serves each job of the batch: it classes no job, and the
// run waits for its retry. A lookup that did not end keeps what it
// learned for a later batch; the inputs are then unknown.
func (t *trust) lookupIn(b *batch, owner, repo string, run int64, messageEvent string, since time.Time) (trustInputs, bool, lookupState, string) {
	if r, ok := b.runs[run]; ok && (r.st != final || r.at.After(since)) {
		return r.in, r.whole, r.st, r.why
	}
	t.mu.Lock()
	defer t.mu.Unlock()
	in, whole, at, err := t.read(b, owner, repo, run, messageEvent, since)
	st, why := final, ""
	switch {
	case err == nil:
	case errors.Is(err, errThrottled):
		st, why = throttled, err.Error()
	case errors.Is(err, errNotReached):
		return in, false, notReached, err.Error()
	default:
		st, why = retry, strings.TrimPrefix(err.Error(), errNoAnswer.Error()+": ")
	}
	b.runs[run] = batchRun{in, whole, st, why, at}
	return in, whole, st, why
}

// read gives the inputs of a run from its run object (only it names the
// attempt), and the time that the read of the object began.
func (t *trust) read(b *batch, owner, repo string, run int64, messageEvent string, since time.Time) (trustInputs, bool, time.Time, error) {
	unknown := trustInputs{event: eventWord(messageEvent), headIsFork: "unknown", actorClass: "unknown", visibility: "unknown"}
	r, at, err := t.runObject(b, owner, repo, run, since)
	if err != nil || r == nil {
		return unknown, false, at, err
	}
	attempt, counted := r["run_attempt"].(float64)
	// The key is the attempt of an object read after the job came: the
	// inputs of an older attempt never serve the job.
	key := runKey{run, int64(attempt)}
	if v, ok := t.cache[key]; ok && counted && time.Since(v.at) < cacheTTL {
		return v.in, true, at, nil
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
		return unknown, false, at, err
	}
	switch v, ok := r["triggering_actor"]; {
	case ok && v != nil:
		c, err := t.classOf(b, owner, v)
		if err != nil {
			return unknown, false, at, err
		}
		out.actorClass = stricter(out.actorClass, c)
	case counted && attempt > 1:
		// A re-run with no triggering actor names no one who started it.
		out.actorClass = "unknown"
	}
	whole := out.headIsFork != "unknown" && out.actorClass != "unknown" && out.visibility != "unknown"
	if whole && counted {
		if t.cache == nil || len(t.cache) >= cacheMax {
			t.cache = map[runKey]keptInputs{}
		}
		t.cache[key] = keptInputs{out, time.Now()}
	}
	return out, whole, at, nil
}

// runObject gives the run object, or nil for a final answer with none,
// and the time that its read began. A run object from a read that began
// after the job came is used again with no call: a re-run comes as new
// jobs, after its attempt exists. Else it is a conditional read when the
// run was read before.
func (t *trust) runObject(b *batch, owner, repo string, run int64, since time.Time) (map[string]any, time.Time, error) {
	seen, ok := t.runs[run]
	if ok && seen.at.After(since) && time.Since(seen.at) < partialTTL {
		return seen.obj, seen.at, nil
	}
	at := time.Now()
	status, r, etag, err := t.get(b.ctx, fmt.Sprintf("/repos/%s/%s/actions/runs/%d", url.PathEscape(owner), url.PathEscape(repo), run), seen.etag)
	switch {
	case err != nil:
		return nil, at, err
	case status == http.StatusNotModified && seen.obj != nil:
		r, etag = seen.obj, seen.etag
	case status != http.StatusOK || r == nil:
		return nil, at, nil
	}
	if t.runs == nil || len(t.runs) >= cacheMax {
		t.runs = map[int64]seenRun{}
	}
	t.runs[run] = seenRun{etag, r, at}
	return r, at, nil
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
	if m, ok := t.members[key]; ok && time.Since(m.at) < partialTTL {
		b.members[key] = m.class
		return m.class, nil
	}
	c := "unknown"
	code, _, _, err := t.get(b.ctx, fmt.Sprintf("/orgs/%s/members/%s", url.PathEscape(owner), url.PathEscape(login)), "")
	switch {
	case err != nil:
		return "", err
	case code == http.StatusNoContent:
		c = "member"
	case code == http.StatusNotFound:
		c = "outside"
	}
	b.members[key] = c
	if t.members == nil || len(t.members) >= cacheMax {
		t.members = map[string]keptClass{}
	}
	t.members[key] = keptClass{c, time.Now()}
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
