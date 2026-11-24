package main

import (
	"strconv"
	"strings"
	"testing"
	"time"

	"github.com/actions/scaleset"
)

// cancelSidecarWith is cancelSidecar with its stub session, to send jobs.
func cancelSidecarWith(t *testing.T, api *cancelAPI) (*sidecar, *stubSession) {
	st := newStub()
	s := stubSidecar(t, st)
	s.trust = scopeTrust(api.srv.URL)
	return s, st
}

// jobOfRun is one message with a JobAvailable of the run of o/r.
func jobOfRun(id int, run int64) *scaleset.RunnerScaleSetMessage {
	return &scaleset.RunnerScaleSetMessage{MessageID: id, JobAvailableMessages: []*scaleset.JobAvailable{{JobMessageBase: scaleset.JobMessageBase{
		RunnerRequestID: int64(id), JobID: "g-" + strconv.Itoa(id), OwnerName: "o", RepositoryName: "r",
		WorkflowRunID: run, EventName: "push",
	}}}}
}

// Review P2-2: the cancel endpoint takes a run id. A retry after a re-run
// (the attempt 2, which podrunner can take) sends no cancel.
func TestARetryAfterAReRunSendsNoCancel(t *testing.T) {
	fastRetries(t, time.Minute)
	api := newCancelAPI(t, 502, 202)
	s := cancelSidecar(t, api)
	s.cancelRefused(refusedJob(), "tier-mismatch")
	api.set(2, "queued")
	out := stderrOf(t, func() {
		nextUntil(t, s, func() bool { return len(s.cancelRetry) == 0 })
	})
	if p, _ := api.count(); p != 1 || !strings.Contains(out, "the attempt 2 runs now") {
		t.Fatalf("%d posts, log %q", p, out)
	}
}

// Review P2-2: a job offered past the limits has no attempt. A retry
// takes the attempt that runs only when it began before the refusal: a
// re-run that began after it gets no cancel.
func TestACancelWithNoAttemptDoesNotCancelALaterReRun(t *testing.T) {
	fastRetries(t, time.Minute)
	for _, c := range []struct {
		name  string
		start time.Duration
		posts int
	}{{"the refused attempt", -time.Minute, 2}, {"a later re-run", time.Second, 1}} {
		api := newCancelAPI(t, 502, 202)
		s := cancelSidecar(t, api)
		s.cancelRefused(&job{owner: "o", repo: "r", run: 5}, "tier-mismatch")
		api.mu.Lock()
		api.attempt, api.started = 2, time.Now().Add(c.start).UTC().Format(time.RFC3339)
		api.mu.Unlock()
		nextUntil(t, s, func() bool { return len(s.cancelRetry) == 0 })
		if p, _ := api.count(); p != c.posts {
			t.Errorf("%s: %d posts", c.name, p)
		}
	}
}

// Review P2-2: a run that ended before the retry gets no cancel, and the
// attempt is settled.
func TestARetryOfACompletedRunSendsNoCancel(t *testing.T) {
	fastRetries(t, time.Minute)
	api := newCancelAPI(t, 502, 202)
	s := cancelSidecar(t, api)
	s.cancelRefused(refusedJob(), "tier-mismatch")
	api.set(1, "completed")
	nextUntil(t, s, func() bool { return len(s.cancelRetry) == 0 })
	if p, _ := api.count(); p != 1 || !isCancelled(s, 5, 1) {
		t.Fatalf("%d posts, cancelled %v", p, isCancelled(s, 5, 1))
	}
}

// Review P2-2: the offer of a job of a newer attempt ends the cancel of an
// older one at once, before its retry is due.
func TestAJobOfANewerAttemptEndsTheCancel(t *testing.T) {
	fastRetries(t, time.Minute)
	retryFirst, retryMax = time.Hour, time.Hour
	api := newCancelAPI(t, 502, 202)
	s, st := cancelSidecarWith(t, api)
	s.cancelRefused(refusedJob(), "tier-mismatch")
	api.set(2, "queued")
	waitPoll(t, st)
	st.put <- jobOfRun(1, 5)
	_, classes := nextClasses(t, s, 1, "5")
	if p, _ := api.count(); len(classes) != 1 || classes[0] != "member" || len(s.cancelRetry) != 0 || p != 1 {
		t.Fatalf("offers %v, list %d, %d posts", classes, len(s.cancelRetry), p)
	}
}

// Review P2-2: an attempt is cancelled once, not a run: a refused job of
// the attempt 2 after the attempt 1 was cancelled is cancelled.
func TestARefusedJobOfALaterAttemptIsCancelled(t *testing.T) {
	fastRetries(t, time.Minute)
	api := newCancelAPI(t, 202)
	s := cancelSidecar(t, api)
	s.cancelRefused(refusedJob(), "tier-mismatch")
	s.cancelRefused(refusedJob(), "tier-mismatch")
	s.cancelRefused(&job{owner: "o", repo: "r", run: 5, attempt: 2}, "tier-mismatch")
	if p, _ := api.count(); p != 2 || !isCancelled(s, 5, 2) {
		t.Fatalf("%d posts, cancelled %v", p, isCancelled(s, 5, 2))
	}
}

// Review P2-3: many cancels that fail use at most their share of the
// trust budget (20 of 100 calls), and a job is still offered.
func TestTheRetriesOfCancelsKeepToTheirShare(t *testing.T) {
	fastRetries(t, time.Hour)
	api := newCancelAPI(t, 502)
	s, st := cancelSidecarWith(t, api)
	s.trust.budget = 100
	for run := int64(1); run <= 30; run++ {
		s.cancelRefused(&job{owner: "o", repo: "r", run: run, attempt: 1}, "tier-mismatch")
	}
	first := api.calls()
	out := stderrOf(t, func() {
		for end := time.Now().Add(500 * time.Millisecond); time.Now().Before(end); time.Sleep(5 * time.Millisecond) {
			nextOnce(s)
		}
	})
	if n := api.calls() - first; n > 20 || n < 10 || len(s.cancelRetry) != 30 {
		t.Fatalf("the retries made %d calls, list %d", n, len(s.cancelRetry))
	}
	if strings.Count(out, "the retries of cancels wait") != 1 {
		t.Fatalf("the log: %q", out)
	}
	waitPoll(t, st)
	st.put <- jobOfRun(1, 99)
	if _, classes := nextClasses(t, s, 1, "5"); len(classes) != 1 || classes[0] != "member" {
		t.Fatalf("the offer: %v", classes)
	}
}

// Review P2-3: after some calls the waits of a cancel grow to 5 minutes,
// not 60 s: a long outage makes few calls.
func TestTheWaitsOfACancelGrowToFiveMinutes(t *testing.T) {
	var step time.Duration
	var steps []time.Duration
	for tries := 1; tries <= 10; tries++ {
		step = cancelStep(step, tries)
		steps = append(steps, step)
	}
	if steps[2] > retryMax || steps[9] != 5*time.Minute || steps[6] <= retryMax {
		t.Fatalf("steps %v", steps)
	}
}

// Review P2-4: the pending cancels are in memory. At the end of stdin and
// at a signal (both call closeSession) each one gets a line.
func TestThePendingCancelsAreNamedAtTheEnd(t *testing.T) {
	fastRetries(t, time.Minute)
	for _, path := range []string{"stdin", "signal"} {
		api := newCancelAPI(t, 502)
		s := cancelSidecar(t, api)
		s.closer = &stubCloser{}
		s.cancelRefused(refusedJob(), "tier-mismatch")
		s.cancelRefused(&job{owner: "o", repo: "r2", run: 6, attempt: 3}, "tier-mismatch")
		out := stderrOf(t, func() {
			if path == "stdin" {
				runSidecar(s, strings.NewReader(""), &strings.Builder{})
			} else {
				s.closeSession()
			}
		})
		for _, w := range []string{"run 5 (attempt 1) of o/r ends with the sidecar; calls: 1;", "run 6 (attempt 3) of o/r2 ends with the sidecar"} {
			if !strings.Contains(out, w) {
				t.Errorf("%s: no %q in %q", path, w, out)
			}
		}
	}
}
