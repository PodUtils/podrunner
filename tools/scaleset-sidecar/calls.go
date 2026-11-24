package main

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"os"
	"time"
)

// A call of GitHub must not hold the wire: one call, and the lookups of
// one `next`, end well inside the reply limit of the mint (15 s,
// REPLY_LIMIT of crates/podrunner-mint/src/sidecar.rs). The engine of
// podrunner calls its sources in turn, so the time of one `next` delays
// each other source and each unit: a short budget, and the results kept
// between calls (trust.go), let a slow run end over some calls.
var (
	lookupTimeout = 4 * time.Second
	nextBudget    = 5 * time.Second
)

// errNoAnswer: a call gave no final answer (no answer in its time, a 5xx,
// a 429, a 403 of a rate limit, a 401, a failed token or sign). The job
// stays queued, and a later `next` tries again.
var errNoAnswer = errors.New("no final answer")

// errThrottled: the budget of the hour is spent, and no call is made. The
// job stays queued until the next hour of the budget.
var errThrottled = errors.New("the trust budget of the hour is spent")

// errNotReached: the time of the `next` ended before the call or during
// it. No fault of GitHub: the job keeps its place.
var errNotReached = errors.New("the time of the next ended")

func noAnswer(why string) error { return fmt.Errorf("%w: %s", errNoAnswer, why) }

// failed gives the error of a call with no answer.
func failed(ctx context.Context, err error) error {
	if ctx.Err() != nil {
		return errNotReached
	}
	return noAnswer(classOf(err))
}

// logf writes one line on stderr. podrunner gives the sidecar its own
// stderr, so the line goes to the log of the service.
func logf(format string, a ...any) {
	fmt.Fprintf(os.Stderr, "podrunner-source-github-scaleset: "+format+"\n", a...)
}

// spend counts one call of GitHub in the budget of the hour. With gate,
// a call past the budget is refused. A conditional read can use a tenth
// more: its 304 is free, so the runs known before go on.
func (t *trust) spend(gate, conditional bool) bool {
	if time.Since(t.hourStart) >= time.Hour {
		t.hourStart, t.used, t.spentSaid = time.Now(), 0, false
	}
	limit := t.budget
	if conditional {
		limit += t.budget / 10
	}
	if gate && t.budget > 0 && t.used >= limit {
		t.Throttled++
		if !t.spentSaid {
			t.spentSaid = true
			logf("the trust budget of the hour is spent (%d calls): new lookups wait until %s",
				t.used, t.hourStart.Add(time.Hour).UTC().Format(time.RFC3339))
		}
		return false
	}
	t.used++
	return true
}

func (t *trust) installationToken(ctx context.Context) (string, error) {
	if t.pat {
		tok, err := t.signer.pat(ctx)
		if err != nil {
			return "", failed(ctx, err)
		}
		return tok, nil
	}
	if t.token != "" && time.Until(t.expires) > 5*time.Minute {
		return t.token, nil
	}
	jwt, err := t.signer.token(ctx)
	if err != nil {
		return "", failed(ctx, err)
	}
	req, _ := http.NewRequestWithContext(ctx, http.MethodPost,
		fmt.Sprintf("%s/app/installations/%d/access_tokens", t.api, t.installation), nil)
	req.Header.Set("Authorization", "Bearer "+jwt)
	req.Header.Set("Accept", "application/vnd.github+json")
	t.spend(false, false)
	resp, err := t.http.Do(req)
	if err != nil {
		return "", failed(ctx, err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusCreated {
		return "", noAnswer(fmt.Sprintf("the installation token: HTTP %d", resp.StatusCode))
	}
	var v struct {
		Token     string    `json:"token"`
		ExpiresAt time.Time `json:"expires_at"`
	}
	if err := json.NewDecoder(io.LimitReader(resp.Body, 1<<16)).Decode(&v); err != nil || v.Token == "" {
		return "", noAnswer("the installation token: no token")
	}
	t.token, t.expires = v.Token, v.ExpiresAt
	return t.token, nil
}

// rateLimited: a 403 of the primary or a secondary rate limit of GitHub,
// not of a missing permission.
func rateLimited(h http.Header) bool {
	return h.Get("Retry-After") != "" || h.Get("X-RateLimit-Remaining") == "0"
}

// get makes one GET of the REST API in the budget. With etag, GitHub
// answers 304 when the object did not change: a 304 is not counted. An
// error is errNoAnswer, errThrottled or errNotReached: each other answer
// (a 404, a 403 of a permission, a body that is not JSON) is final.
func (t *trust) get(ctx context.Context, path, etag string) (int, map[string]any, string, error) {
	if ctx.Err() != nil {
		return 0, nil, "", errNotReached
	}
	token, err := t.installationToken(ctx)
	if err != nil {
		return 0, nil, "", err
	}
	if !t.spend(true, etag != "") {
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
	code := resp.StatusCode
	switch {
	case code == http.StatusNotModified:
		t.used--
		return code, nil, etag, nil
	case code == http.StatusUnauthorized:
		// The installation token ended or was revoked: get a new one.
		t.token = ""
		return code, nil, "", noAnswer("HTTP 401")
	case code >= 500, code == http.StatusTooManyRequests, code == http.StatusForbidden && rateLimited(resp.Header):
		return code, nil, "", noAnswer(fmt.Sprintf("HTTP %d", code))
	}
	var v map[string]any
	if err := json.NewDecoder(io.LimitReader(resp.Body, 1<<20)).Decode(&v); err != nil && err != io.EOF {
		var syntax *json.SyntaxError
		var typ *json.UnmarshalTypeError
		if !errors.As(err, &syntax) && !errors.As(err, &typ) {
			// The body was cut: the connection, or the time.
			return 0, nil, "", failed(ctx, err)
		}
		v = nil
	}
	return code, v, resp.Header.Get("ETag"), nil
}
