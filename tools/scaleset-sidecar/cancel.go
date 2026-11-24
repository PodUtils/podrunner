package main

import (
	"context"
	"fmt"
	"io"
	"net/http"
	"strings"
	"time"
)

// A job that podrunner refuses stays assigned to the scale set at GitHub,
// and the set counts it as busy: one refused pull request of a fork stops
// a trusted set (T-169). The scale-set API gives no job back, so the
// sidecar cancels the run of a refused job (the owner, Q7, 2026-11-15).
// The App needs "Actions: read and write".
//
// A cancel that fails stays in a short list and is tried again by `next`,
// as a refused job that is never cancelled keeps the set busy (T-623).
var (
	// cancelMaxAge: past it a cancel is given up. The retry waits are
	// retryFirst doubled to retryMax (queue.go).
	cancelMaxAge = 30 * time.Minute
	// cancelListMax bounds the list: a flood of refusals during an outage
	// must not grow the memory.
	cancelListMax = 256
)

// cancelTry is a cancel that has not ended.
type cancelTry struct {
	run         int64
	owner, repo string
	first, at   time.Time
	step        time.Duration
	tries       int
}

// cancelOutcome: what an answer of the cancel endpoint means.
type cancelOutcome int

const (
	// cancelAgain: no final answer; try again later.
	cancelAgain cancelOutcome = iota
	// cancelDone: the run is cancelled, or it ended before.
	cancelDone
	// cancelFinal: a final answer that a retry does not change.
	cancelFinal
)

// outcomeOf classes the answer of `POST .../actions/runs/N/cancel`: 202
// is done; 409 is done when GitHub says that the run is complete or
// cancelled; a 5xx, a 429, a 401 and no answer are tried again. A 401 of
// a PAT is final: only podrunner can give a new one (T-624).
func outcomeOf(code int, note string, pat bool) cancelOutcome {
	switch {
	case code == http.StatusAccepted:
		return cancelDone
	case code == http.StatusConflict:
		if strings.Contains(note, "complete") || strings.Contains(note, "cancel") {
			return cancelDone
		}
		return cancelAgain
	case code == http.StatusUnauthorized && pat:
		return cancelFinal
	case code == 0, code >= 500, code == http.StatusUnauthorized, code == http.StatusTooManyRequests, code == http.StatusRequestTimeout:
		return cancelAgain
	}
	return cancelFinal
}

// cancelRefused cancels the run of `j` when `reason` is a refusal. A wait
// that ran out, a job that GitHub cancelled, and a test of the conformance
// suite cancel nothing; each run is cancelled once.
func (s *sidecar) cancelRefused(j *job, reason string) {
	switch reason {
	case "", "wait-deadline", "cancelled", "conformance":
		return
	}
	if j.run == 0 || j.owner == "" || j.repo == "" || s.trust == nil {
		return
	}
	if s.cancelled[j.run] {
		return
	}
	for _, c := range s.cancelRetry {
		if c.run == j.run {
			return
		}
	}
	c := &cancelTry{run: j.run, owner: j.owner, repo: j.repo, first: time.Now()}
	ctx, cancel := context.WithTimeout(s.ctx, lookupTimeout)
	defer cancel()
	if !s.tryCancel(ctx, c, reason) {
		s.keepCancel(c)
	}
}

// tryCancel makes one call. True when the cancel has ended (done, a final
// answer, or given up); else c has its next time.
func (s *sidecar) tryCancel(ctx context.Context, c *cancelTry, reason string) bool {
	code, note := s.trust.post(ctx, fmt.Sprintf("/repos/%s/%s/actions/runs/%d/cancel", c.owner, c.repo, c.run))
	c.tries++
	out := outcomeOf(code, note, s.trust.pat)
	// A line for the first call, the second (the first retry), and each end.
	if reason != "" || c.tries == 2 || out != cancelAgain {
		why := "retry"
		if reason != "" {
			why = "refused (" + reason + ")"
		}
		more := ""
		switch out {
		case cancelAgain:
			more = "; it is tried again"
		case cancelFinal:
			more = "; it is not tried again"
		}
		logf("cancel the run %d of %s/%s: %s: HTTP %d%s", c.run, c.owner, c.repo, why, code, more)
	}
	switch out {
	case cancelDone:
		s.markCancelled(c.run)
		return true
	case cancelFinal:
		// No retry changes it (a missing permission, a run that is gone):
		// the run is settled, so its other jobs make no call.
		s.markCancelled(c.run)
		return true
	}
	now := time.Now()
	if now.Sub(c.first) >= cancelMaxAge {
		s.giveUpCancel(c, now)
		return true
	}
	c.step = min(max(c.step*2, retryFirst), retryMax)
	c.at = now.Add(c.step)
	return false
}

func (s *sidecar) markCancelled(run int64) {
	if s.cancelled == nil {
		s.cancelled = map[int64]bool{}
	}
	s.cancelled[run] = true
}

// keepCancel adds c to the list; the oldest goes when the list is full.
func (s *sidecar) keepCancel(c *cancelTry) {
	if len(s.cancelRetry) >= cancelListMax {
		s.giveUpCancel(s.cancelRetry[0], time.Now())
		s.cancelRetry = s.cancelRetry[1:]
	}
	s.cancelRetry = append(s.cancelRetry, c)
}

func (s *sidecar) giveUpCancel(c *cancelTry, now time.Time) {
	logf("cancel the run %d of %s/%s: given up after %d calls over %s: the refused job may keep the scale set busy",
		c.run, c.owner, c.repo, c.tries, now.Sub(c.first).Round(time.Second))
}

// retryCancels makes the calls that are due, in the time of ctx. A call
// that ctx ends is not counted as a refusal of GitHub.
func (s *sidecar) retryCancels(ctx context.Context) {
	if len(s.cancelRetry) == 0 {
		return
	}
	now := time.Now()
	kept := make([]*cancelTry, 0, len(s.cancelRetry))
	for _, c := range s.cancelRetry {
		switch {
		case now.Sub(c.first) >= cancelMaxAge:
			s.giveUpCancel(c, now)
		case ctx.Err() != nil || now.Before(c.at):
			kept = append(kept, c)
		case !s.tryCancel(ctx, c, ""):
			kept = append(kept, c)
		}
	}
	s.cancelRetry = kept
}

// post makes one POST of the REST API with the token of the installation;
// its HTTP status (0 with no answer) and the start of the answer in lower
// case. It is counted in the budget, and never refused by it: a refused
// job must free the scale set. A 403 of a rate limit is a 429. A 401 drops
// the token, as `get` does.
func (t *trust) post(ctx context.Context, path string) (int, string) {
	t.mu.Lock()
	defer t.mu.Unlock()
	token, err := t.installationToken(ctx)
	if err != nil {
		return 0, ""
	}
	t.spend(false, false)
	req, _ := http.NewRequestWithContext(ctx, http.MethodPost, t.api+path, nil)
	req.Header.Set("Authorization", "Bearer "+token)
	req.Header.Set("Accept", "application/vnd.github+json")
	resp, err := t.http.Do(req)
	if err != nil {
		return 0, ""
	}
	defer resp.Body.Close()
	body, _ := io.ReadAll(io.LimitReader(resp.Body, 4096))
	code := resp.StatusCode
	switch {
	case code == http.StatusUnauthorized:
		t.token = ""
	case code == http.StatusForbidden && rateLimited(resp.Header):
		code = http.StatusTooManyRequests
	}
	return code, strings.ToLower(string(body))
}
