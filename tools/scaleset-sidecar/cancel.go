package main

import (
	"context"
	"fmt"
	"net/http"
	"os"
)

// A job that podrunner refuses stays assigned to the scale set at GitHub,
// and the set counts it as busy: one refused pull request of a fork stops
// a trusted set (T-169). The scale-set API gives no job back, so the
// sidecar cancels the run of a refused job (the owner, Q7, 2026-11-15).
// The App needs "Actions: read and write".

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
	if s.cancelled == nil {
		s.cancelled = map[int64]bool{}
	}
	if s.cancelled[j.run] {
		return
	}
	s.cancelled[j.run] = true
	code := s.trust.post(fmt.Sprintf("/repos/%s/%s/actions/runs/%d/cancel", j.owner, j.repo, j.run))
	fmt.Fprintf(os.Stderr, "podrunner-source-github-scaleset: cancel the run %d of %s/%s: refused (%s): HTTP %d\n",
		j.run, j.owner, j.repo, reason, code)
}

// post makes one POST of the REST API with the token of the installation;
// its HTTP status, or 0 with no answer. It is counted in the budget, and
// never refused by it: a refused job must free the scale set.
func (t *trust) post(path string) int {
	t.mu.Lock()
	defer t.mu.Unlock()
	token, err := t.installationToken(context.Background())
	if err != nil {
		return 0
	}
	t.spend(false, false)
	req, _ := http.NewRequest(http.MethodPost, t.api+path, nil)
	req.Header.Set("Authorization", "Bearer "+token)
	req.Header.Set("Accept", "application/vnd.github+json")
	resp, err := t.http.Do(req)
	if err != nil {
		return 0
	}
	resp.Body.Close()
	return resp.StatusCode
}
