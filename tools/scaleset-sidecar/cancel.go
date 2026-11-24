package main

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/url"
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
// The cancel endpoint takes a run id, not an attempt: a retry reads the
// run first, so that it does not cancel a re-run that podrunner took.
var (
	// cancelMaxAge: past it a cancel is given up. The retry waits are
	// retryFirst doubled to retryMax (queue.go), and to cancelSlowMax
	// after cancelSlowAfter calls: a long outage must not spend the budget.
	cancelMaxAge    = 30 * time.Minute
	cancelSlowAfter = 4
	cancelSlowMax   = 5 * time.Minute
	// cancelListMax bounds the list: a flood of refusals during an outage
	// must not grow the memory.
	cancelListMax = 256
)

const (
	// cancelShare: the retries use at most this percent of the trust
	// budget of the hour, so the offers keep their lookups.
	cancelShare = 20
	// cancelCallsMax: the most calls of one retry (a token, the read of
	// the run, the POST).
	cancelCallsMax = 3
)

// cancelTry is a cancel that has not ended. attempt: the attempt of the
// refused job, 0 when not known (the first read of the run sets it).
type cancelTry struct {
	run, attempt int64
	owner, repo  string
	first, at    time.Time
	step         time.Duration
	tries        int
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

// outcomeOf classes the answer of `POST .../actions/runs/N/cancel` and its
// message (lower case): 202 is done; a 409 is done only when GitHub says
// that the run is completed. Each 409 of the endpoint starts "Cannot
// cancel a workflow run", also for a run that is not queued yet, which is
// not done. A 5xx, a 429, a 401 and no answer are tried again. A 401 of a
// PAT is final: only podrunner can give a new one (T-624).
func outcomeOf(code int, msg string, pat bool) cancelOutcome {
	switch {
	case code == http.StatusAccepted:
		return cancelDone
	case code == http.StatusConflict:
		if strings.Contains(msg, "that is completed") {
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
// suite cancel nothing; each attempt of a run is cancelled once.
func (s *sidecar) cancelRefused(j *job, reason string) {
	switch reason {
	case "", "wait-deadline", "cancelled", "conformance":
		return
	}
	if j.run == 0 || j.owner == "" || j.repo == "" || s.trust == nil {
		return
	}
	attempt := j.attempt
	if attempt == 0 {
		// A job offered past the limits has no attempt of its own.
		attempt = s.trust.attemptOf(j.run)
	}
	if s.cancelled[runKey{j.run, attempt}] {
		return
	}
	for _, c := range s.cancelRetry {
		if c.run == j.run && c.attempt == attempt {
			return
		}
	}
	c := &cancelTry{run: j.run, attempt: attempt, owner: j.owner, repo: j.repo, first: time.Now()}
	ctx, cancel := context.WithTimeout(s.ctx, lookupTimeout)
	defer cancel()
	if !s.tryCancel(ctx, c, reason) {
		s.keepCancel(c)
	}
	s.keepPending()
}

// tryCancel makes one call. True when the cancel has ended (done, a final
// answer, or given up); else c has its next time.
func (s *sidecar) tryCancel(ctx context.Context, c *cancelTry, reason string) bool {
	code, msg := s.trust.post(ctx, fmt.Sprintf("/repos/%s/%s/actions/runs/%d/cancel",
		url.PathEscape(c.owner), url.PathEscape(c.repo), c.run))
	if code == 0 && ctx.Err() != nil {
		// The time of the `next` ended, not GitHub: no try, the same wait.
		if reason != "" {
			s.logCancel(c, "refused ("+reason+")", code, cancelAgain)
		}
		return s.giveUpPastAge(c, time.Now())
	}
	c.tries++
	out := outcomeOf(code, msg, s.trust.pat)
	// A line for the first call, the second (the first retry), and each end.
	if reason != "" || c.tries == 2 || out != cancelAgain {
		why := "retry"
		if reason != "" {
			why = "refused (" + reason + ")"
		}
		s.logCancel(c, why, code, out)
	}
	switch out {
	case cancelDone, cancelFinal:
		// A final answer is no retry either (a missing permission, a run
		// that is gone): the attempt is settled, so its other jobs make
		// no call.
		s.markCancelled(c)
		return true
	}
	return s.again(c)
}

func (s *sidecar) logCancel(c *cancelTry, why string, code int, out cancelOutcome) {
	more := ""
	switch out {
	case cancelAgain:
		more = "; it is tried again"
	case cancelFinal:
		more = "; it is not tried again"
	}
	logf("cancel the run %d (attempt %d) of %s/%s: %s: HTTP %d%s", c.run, c.attempt, c.owner, c.repo, why, code, more)
}

// again sets the next time of a cancel that got no final answer, or
// gives it up past cancelMaxAge (true).
func (s *sidecar) again(c *cancelTry) bool {
	now := time.Now()
	if s.giveUpPastAge(c, now) {
		return true
	}
	c.step = cancelStep(c.step, c.tries)
	c.at = now.Add(c.step)
	return false
}

// cancelStep gives the next wait of a cancel after `tries` calls.
func cancelStep(step time.Duration, tries int) time.Duration {
	most := retryMax
	if tries >= cancelSlowAfter {
		most = max(retryMax, cancelSlowMax)
	}
	return min(max(step*2, retryFirst), most)
}

func (s *sidecar) giveUpPastAge(c *cancelTry, now time.Time) bool {
	if now.Sub(c.first) < cancelMaxAge {
		return false
	}
	s.giveUpCancel(c, now)
	return true
}

func (s *sidecar) markCancelled(c *cancelTry) {
	if s.cancelled == nil {
		s.cancelled = map[runKey]bool{}
	}
	s.cancelled[runKey{c.run, c.attempt}] = true
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
	logf("cancel the run %d (attempt %d) of %s/%s: given up after %d calls over %s: the refused job may keep the scale set busy",
		c.run, c.attempt, c.owner, c.repo, c.tries, now.Sub(c.first).Round(time.Second))
}

// retryCancels makes the calls that are due, in the time of ctx and in the
// share of the budget. A call that ctx ends, or that the budget does not
// allow, is no try.
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
		case ctx.Err() != nil || now.Before(c.at) || !s.trust.cancelRoom():
			kept = append(kept, c)
		case !s.retryCancel(ctx, c):
			kept = append(kept, c)
		}
	}
	s.cancelRetry = kept
	s.keepPending()
}

// retryCancel reads the run, then cancels it when the refused attempt
// still runs. True when the cancel has ended.
func (s *sidecar) retryCancel(ctx context.Context, c *cancelTry) bool {
	s.trust.setCancelling(true)
	defer s.trust.setCancelling(false)
	obj, err := s.trust.runNow(ctx, c.owner, c.repo, c.run)
	switch {
	case errors.Is(err, errThrottled), errors.Is(err, errNotReached):
		return false
	case err != nil:
		c.tries++
		return s.again(c)
	case obj != nil:
		n, _ := obj["run_attempt"].(float64)
		attempt := int64(n)
		if c.attempt == 0 && !startedAfter(obj, c.first) {
			// The attempt that runs began before the refusal: it is the
			// refused one.
			c.attempt = attempt
		}
		status, _ := obj["status"].(string)
		switch {
		case attempt != 0 && attempt != c.attempt:
			logf("cancel the run %d (attempt %d) of %s/%s: the attempt %d runs now: the cancel ends",
				c.run, c.attempt, c.owner, c.repo, attempt)
			return true
		case status == "completed":
			logf("cancel the run %d (attempt %d) of %s/%s: the run is completed: the cancel ends",
				c.run, c.attempt, c.owner, c.repo)
			s.markCancelled(c)
			return true
		}
	}
	// A final answer with no run object: the POST gives the final answer.
	return s.tryCancel(ctx, c, "")
}

// startedAfter: the attempt of the run object began after t (a re-run).
func startedAfter(obj map[string]any, t time.Time) bool {
	v, _ := obj["run_started_at"].(string)
	at, err := time.Parse(time.RFC3339, v)
	return err == nil && at.After(t)
}

// dropOlderCancels ends the cancels of older attempts of a run when a job
// of `attempt` is offered: podrunner can take that job, and a cancel of
// the run would cancel it.
func (s *sidecar) dropOlderCancels(run, attempt int64) {
	if attempt == 0 || len(s.cancelRetry) == 0 {
		return
	}
	kept := make([]*cancelTry, 0, len(s.cancelRetry))
	for _, c := range s.cancelRetry {
		if c.run == run && c.attempt < attempt {
			logf("cancel the run %d (attempt %d) of %s/%s: a job of the attempt %d is offered: the cancel ends",
				c.run, c.attempt, c.owner, c.repo, attempt)
			continue
		}
		kept = append(kept, c)
	}
	s.cancelRetry = kept
	s.keepPending()
}

// keepPending keeps a line for each pending cancel for closeSession, which
// can run in the goroutine of a signal.
func (s *sidecar) keepPending() {
	lines := make([]string, 0, len(s.cancelRetry))
	for _, c := range s.cancelRetry {
		lines = append(lines, fmt.Sprintf("the cancel of the run %d (attempt %d) of %s/%s ends with the sidecar; calls: %d; "+
			"the refused job may keep the scale set busy", c.run, c.attempt, c.owner, c.repo, c.tries))
	}
	s.heldMu.Lock()
	s.pendings = lines
	s.heldMu.Unlock()
}

// post makes one POST of the REST API with the token of the installation;
// its HTTP status (0 with no answer) and the message of the answer in
// lower case. It is counted in the budget, and never refused by it: a
// refused job must free the scale set (retryCancels keeps the retries in
// a share). A 403 of a rate limit is a 429. A 401 drops the token, as
// `get` does.
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
	var v struct {
		Message string `json:"message"`
	}
	_ = json.NewDecoder(io.LimitReader(resp.Body, 4096)).Decode(&v)
	msg := strings.ToLower(v.Message)
	code := resp.StatusCode
	switch {
	case code == http.StatusUnauthorized:
		t.token = ""
	case code == http.StatusForbidden && (rateLimited(resp.Header) || rateLimitMessage(msg)):
		code = http.StatusTooManyRequests
	}
	return code, msg
}

// runNow reads the run object now (a conditional read when it was read
// before), in the gate of the budget. nil: a final answer with no object.
func (t *trust) runNow(ctx context.Context, owner, repo string, run int64) (map[string]any, error) {
	t.mu.Lock()
	defer t.mu.Unlock()
	obj, _, err := t.runObject(newBatch(ctx), owner, repo, run, time.Now())
	return obj, err
}

// attemptOf gives the attempt of the last run object read, 0 when none.
func (t *trust) attemptOf(run int64) int64 {
	t.mu.Lock()
	defer t.mu.Unlock()
	n, _ := t.runs[run].obj["run_attempt"].(float64)
	return int64(n)
}

func (t *trust) setCancelling(on bool) {
	t.mu.Lock()
	defer t.mu.Unlock()
	t.cancelling = on
}

// cancelRoom: a retry can make its calls in the share of the budget, and
// the budget keeps them. Else the sidecar says so once in the hour.
func (t *trust) cancelRoom() bool {
	t.mu.Lock()
	defer t.mu.Unlock()
	t.roll()
	if t.budget <= 0 {
		return true
	}
	share := max(t.budget*cancelShare/100, cancelCallsMax)
	if t.cancelUsed+cancelCallsMax <= share && t.used+cancelCallsMax <= t.budget {
		return true
	}
	if !t.cancelSaid {
		t.cancelSaid = true
		logf("the retries of cancels wait until %s: they used %d calls of the trust budget of the hour (their share is %d), "+
			"and %d calls of the budget are left", t.hourStart.Add(time.Hour).UTC().Format(time.RFC3339), t.cancelUsed, share,
			max(t.budget-t.used, 0))
	}
	return false
}
