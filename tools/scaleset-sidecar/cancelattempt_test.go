package main

import (
	"strings"
	"testing"
	"time"
)

// unknownJob is a job offered past the limits: its run was never read, so
// it has no attempt. It came at `since`.
func unknownJob(since time.Time) *job {
	return &job{owner: "o", repo: "r", run: 5, since: since}
}

// T-625 item 1: a cancel with no attempt is not older than an attempt. A
// job of the attempt 1 of the same run does not end it with no POST; the
// retry reads the run and decides.
func TestACancelWithNoAttemptStaysWhenAJobOfAttempt1IsOffered(t *testing.T) {
	fastRetries(t, time.Minute)
	retryFirst, retryMax = time.Hour, time.Hour
	api := newCancelAPI(t, 502, 202)
	s, st := cancelSidecarWith(t, api)
	s.cancelRefused(unknownJob(time.Now()), "tier-mismatch")
	waitPoll(t, st)
	st.put <- jobOfRun(1, 5)
	_, classes := nextClasses(t, s, 1, "5")
	if len(classes) != 1 || len(s.cancelRetry) != 1 || s.cancelRetry[0].attempt != 0 {
		t.Fatalf("offers %v, list %d", classes, len(s.cancelRetry))
	}
}

// T-625 item 6: the log lines name an attempt that is not known "unknown",
// not 0.
func TestTheLogNamesAnUnknownAttempt(t *testing.T) {
	fastRetries(t, time.Minute)
	retryFirst, retryMax = time.Hour, time.Hour
	api := newCancelAPI(t, 502)
	s := cancelSidecar(t, api)
	out := stderrOf(t, func() { s.cancelRefused(unknownJob(time.Now()), "tier-mismatch") })
	if !strings.Contains(out, "(attempt unknown)") || strings.Contains(out, "(attempt 0)") {
		t.Fatalf("the log: %q", out)
	}
	s.keepPending()
	if p := strings.Join(s.pendings, "\n"); !strings.Contains(p, "(attempt unknown)") {
		t.Fatalf("pending %q", p)
	}
	if out := stderrOf(t, func() { s.giveUpCancel(s.cancelRetry[0], time.Now()) }); !strings.Contains(out, "(attempt unknown)") {
		t.Fatalf("the give-up line: %q", out)
	}
}

// T-625 item 2: a kept run object that was read before the job came says
// nothing of the attempt of the job. It gives no attempt, so a `cancelled`
// key of its attempt does not skip the cancel, and the cancel keeps no
// wrong attempt for its retry.
func TestAStaleRunObjectGivesNoAttemptForANewerJob(t *testing.T) {
	fastRetries(t, time.Minute)
	stale := func(s *sidecar, age time.Duration) {
		s.trust.runs = map[int64]seenRun{5: {etag: "x", obj: map[string]any{"run_attempt": float64(1)}, at: time.Now().Add(-age)}}
	}
	t.Run("a cancelled key of the stale attempt does not skip it", func(t *testing.T) {
		api := newCancelAPI(t, 202)
		s := cancelSidecar(t, api)
		stale(s, time.Minute)
		s.cancelled = map[runKey]bool{{5, 1}: true}
		s.cancelRefused(unknownJob(time.Now()), "tier-mismatch")
		if p, _ := api.count(); p != 1 {
			t.Fatalf("%d posts", p)
		}
	})
	t.Run("a failed first POST keeps no attempt from it", func(t *testing.T) {
		api := newCancelAPI(t, 502, 202)
		s := cancelSidecar(t, api)
		stale(s, time.Minute)
		s.cancelRefused(unknownJob(time.Now()), "tier-mismatch")
		if len(s.cancelRetry) != 1 || s.cancelRetry[0].attempt != 0 {
			t.Fatalf("list %v", s.cancelRetry)
		}
	})
	t.Run("an object read after the job came gives its attempt", func(t *testing.T) {
		api := newCancelAPI(t, 202)
		s := cancelSidecar(t, api)
		stale(s, time.Second)
		s.cancelled = map[runKey]bool{{5, 1}: true}
		s.cancelRefused(unknownJob(time.Now().Add(-time.Minute)), "tier-mismatch")
		if p, _ := api.count(); p != 0 {
			t.Fatalf("%d posts", p)
		}
	})
}

// T-625 item 5: the attempt of a cancel is not known: no key of attempt 0
// is set, in a done answer or in a final one, so a job of the run that
// came after the cancel is cancelled too. A job that came before it is
// covered by it.
func TestACancelWithNoAttemptSetsNoKeyOfAttempt0(t *testing.T) {
	fastRetries(t, time.Minute)
	for _, code := range []int{202, 404} {
		api := newCancelAPI(t, code)
		s := cancelSidecar(t, api)
		before := time.Now()
		s.cancelRefused(unknownJob(before), "tier-mismatch")
		s.cancelRefused(unknownJob(before), "tier-mismatch")
		if p, _ := api.count(); p != 1 || isCancelled(s, 5, 0) || len(s.cancelled) != 0 {
			t.Fatalf("%d: %d posts, keys %v", code, p, s.cancelled)
		}
		s.cancelRefused(unknownJob(time.Now()), "tier-mismatch")
		if p, _ := api.count(); p != 2 {
			t.Fatalf("%d: a later job: %d posts", code, p)
		}
	}
}

// T-625 items 3 and 4: the retry of a cancel with no attempt takes the
// running attempt as the refused one only when it is attempt 1, or began
// before the job came. A start that is not valid is not known. A start
// after the arrival of the job (not after the refusal, a long time later)
// is a re-run. A start inside the skew margin is not known. Where it is
// not known the retry reads once more, then ends with a line and no POST.
func TestTheRetryOfACancelWithNoAttemptJudgesTheStart(t *testing.T) {
	fastRetries(t, time.Minute)
	since := time.Now().Add(-10 * time.Minute)
	at := func(d time.Duration) string { return since.Add(d).UTC().Format(time.RFC3339) }
	for _, c := range []struct {
		name    string
		attempt int
		started string
		posts   int
		log     string
	}{
		{"before the job came", 2, at(-time.Minute), 2, ""},
		{"attempt 1 with no start", 1, "", 2, ""},
		{"no start", 2, "", 1, "not known whether the attempt 2"},
		{"a start that is not valid", 2, "yesterday", 1, "not known whether the attempt 2"},
		{"inside the skew margin", 2, at(30 * time.Second), 1, "not known whether the attempt 2"},
		{"a re-run before the refusal", 2, at(5 * time.Minute), 1, "began after the job came"},
	} {
		api := newCancelAPI(t, 502, 202)
		s := cancelSidecar(t, api)
		s.cancelRefused(unknownJob(since), "tier-mismatch")
		api.mu.Lock()
		api.attempt, api.started = c.attempt, c.started
		api.mu.Unlock()
		out := stderrOf(t, func() { nextUntil(t, s, func() bool { return len(s.cancelRetry) == 0 }) })
		if p, _ := api.count(); p != c.posts || !strings.Contains(out, c.log) {
			t.Errorf("%s: %d posts, log %q", c.name, p, out)
		}
		if isCancelled(s, 5, 0) {
			t.Errorf("%s: a key of attempt 0", c.name)
		}
	}
}
