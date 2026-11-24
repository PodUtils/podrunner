package main

import (
	"strings"
	"testing"
	"time"

	"github.com/actions/scaleset"
)

// P3-5 of the third T-504 review: a job that GitHub ends (cancelled, or
// taken by another set) while it waits in the queue leaves the queue: it
// is never offered and never claimed. podrunner never knew it, so no
// event names it.
func TestACompletedJobLeavesTheQueue(t *testing.T) {
	f, ts := newFakeGitHub(t)
	f.run("5", runObject(user("alice", 1), "", 1))
	st := newStub()
	s := lookupSidecar(t, st, ts.URL)
	waitPoll(t, st)
	st.put <- jobsOf(1, 5, 5, 5)
	if ids, _ := nextClasses(t, s, 0, "5"); len(ids) != 0 || len(s.queue) != 3 {
		t.Fatalf("%v, %d queued", ids, len(s.queue))
	}
	waitPoll(t, st)
	st.put <- &scaleset.RunnerScaleSetMessage{MessageID: 2, JobCompletedMessages: []*scaleset.JobCompleted{
		{Result: "canceled", JobMessageBase: scaleset.JobMessageBase{JobID: "g-1001"}},
		{Result: "succeeded", JobMessageBase: scaleset.JobMessageBase{RunnerRequestID: 1002}},
	}}
	rep := s.handle([]byte(`{"schema":1,"op":"next","capacity":3,"wait_s":5}`))
	if ids := offers(rep); len(ids) != 1 || ids[0] != "1003" || len(rep["events"].([]any)) != 1 {
		t.Fatalf("%v", rep)
	}
	if len(s.byJobID) != 1 {
		t.Fatalf("the jobIds kept: %v", s.byJobID)
	}
}

// P3-6: the inputs of a run, and the class of a login, are kept for a
// time only: a member who leaves the organization is not a member for
// ever.
func TestTheCachesOfTheLookupsExpire(t *testing.T) {
	defer func(a, b time.Duration) { cacheTTL, partialTTL = a, b }(cacheTTL, partialTTL)
	f, ts := newFakeGitHub(t)
	f.run("1", runObject(user("alice", 1), "", 1))
	tr := scopeTrust(ts.URL)
	for i := 0; i < 2; i++ {
		if in, _ := tr.lookup("example-org", "app", 1, "push"); in.actorClass != "member" || f.memberLookups != 1 {
			t.Fatalf("lookup %d: %+v, %d lookups", i, in, f.memberLookups)
		}
	}
	cacheTTL, partialTTL = 50*time.Millisecond, 50*time.Millisecond
	time.Sleep(100 * time.Millisecond)
	f.mu.Lock()
	f.members["alice"] = false
	f.mu.Unlock()
	if in, _ := tr.lookup("example-org", "app", 1, "push"); in.actorClass != "outside" || f.memberLookups != 2 {
		t.Fatalf("after the time: %+v, %d lookups", in, f.memberLookups)
	}
}

// P2-4: the queue is in memory, and GitHub deleted the message of each
// queued job. At its end the sidecar names the jobs that it held, so an
// operator can find them.
func TestTheQueuedJobsAreNamedAtTheEnd(t *testing.T) {
	st := newStub()
	s := stubSidecar(t, st)
	s.closer = &stubCloser{}
	waitPoll(t, st)
	st.put <- jobsOf(1, 5, 6)
	s.handle([]byte(`{"schema":1,"op":"next","capacity":0,"wait_s":5}`))
	log := stderrOf(t, s.closeSession)
	for _, w := range []string{"2 queued jobs", "1001 (run 5 of example-org/app)", "1002 (run 6 of example-org/app)", "deleted"} {
		if !strings.Contains(log, w) {
			t.Errorf("no %q in %q", w, log)
		}
	}
}

// P2-4: at the start of its session the sidecar holds no job. When GitHub
// counts jobs of the set, the sidecar says so: they can be the jobs of an
// earlier sidecar, whose messages GitHub deleted.
func TestTheStartNamesTheJobsThatGitHubCounts(t *testing.T) {
	if l := sessionStartLine(nil); l != "" {
		t.Errorf("no statistics: %q", l)
	}
	if l := sessionStartLine(&scaleset.RunnerScaleSetStatistic{}); l != "" {
		t.Errorf("no jobs: %q", l)
	}
	l := sessionStartLine(&scaleset.RunnerScaleSetStatistic{TotalAvailableJobs: 3, TotalAssignedJobs: 2, TotalAcquiredJobs: 1})
	for _, w := range []string{"3 available", "1 acquired", "2 assigned", "holds no job"} {
		if !strings.Contains(l, w) {
			t.Errorf("no %q in %q", w, l)
		}
	}
}

// P2-1: the counts that the sidecar kept and never showed (the calls past
// the budget, the final answers with no facts) are in the status line on
// stderr, at most once a minute.
func TestTheCountsAreReported(t *testing.T) {
	_, ts := newFakeGitHub(t)
	st := newStub()
	s := lookupSidecar(t, st, ts.URL)
	waitPoll(t, st)
	// Run 9 is not known: a final answer with no facts.
	st.put <- jobsOf(1, 9)
	log := stderrOf(t, func() {
		nextClasses(t, s, 1, "5")
		nextClasses(t, s, 1, "0")
	})
	if strings.Count(log, "the queue holds") != 1 || !strings.Contains(log, "failed 1") || !strings.Contains(log, "throttled 0") {
		t.Fatalf("the log: %q", log)
	}
}
