package main

import (
	"io"
	"os"
	"strconv"
	"strings"
	"testing"
	"time"
)

// stderrOf runs f and gives what it wrote on stderr.
func stderrOf(t *testing.T, f func()) string {
	t.Helper()
	r, w, err := os.Pipe()
	if err != nil {
		t.Fatal(err)
	}
	old := os.Stderr
	os.Stderr = w
	done := make(chan string)
	go func() {
		b, _ := io.ReadAll(r)
		done <- string(b)
	}()
	defer func() {
		os.Stderr = old
		w.Close()
	}()
	f()
	os.Stderr = old
	w.Close()
	return <-done
}

// next sends one `next` and gives the classes of its offers.
func nextClasses(t *testing.T, s *sidecar, capacity int, wait string) ([]string, []string) {
	t.Helper()
	rep := s.handle([]byte(`{"schema":1,"op":"next","capacity":` + strconv.Itoa(capacity) + `,"wait_s":` + wait + `}`))
	return offers(rep), offerClasses(t, rep)
}

// withdrawUnknown does what podrunner does with an offer of a trusted set
// whose inputs are unknown: it refuses it, and the sidecar cancels its run.
func withdrawUnknown(t *testing.T, s *sidecar, ids, classes []string) {
	t.Helper()
	for i, c := range classes {
		if c == "unknown" {
			s.handle([]byte(`{"schema":1,"op":"withdraw","job_id":"` + ids[i] + `","reason":"tier-mismatch"}`))
		}
	}
}

// P2-1 of the third T-504 review: a run whose read does not end stops no
// job behind it. Before, the first pending job ended the `next`: the jobs
// of a run read before waited behind it, and each `next` spent its budget
// on the same run. A run with no final answer waits for its retry with no
// call, so the next `next` is fast.
func TestAHungRunAtTheHeadLeavesTheJobsBehindItOffered(t *testing.T) {
	defer func(a, b time.Duration) { lookupTimeout, nextBudget = a, b }(lookupTimeout, nextBudget)
	lookupTimeout, nextBudget = 300*time.Millisecond, 2*time.Second
	f, ts := newFakeGitHub(t)
	f.run("7", runObject(user("alice", 1), "", 1))
	f.run("8", runObject(user("alice", 1), "", 1))
	st := newStub()
	s := lookupSidecar(t, st, ts.URL)
	waitPoll(t, st)
	st.put <- jobsOf(1, 8)
	if ids, _ := nextClasses(t, s, 1, "5"); len(ids) != 1 {
		t.Fatalf("run 8 first: %v", ids)
	}
	f.slowRun("7", time.Minute)
	waitPoll(t, st)
	st.put <- jobsOf(2, 7, 8)
	ids, classes := nextClasses(t, s, 2, "5")
	if len(ids) != 1 || ids[0] != "2002" || classes[0] != "member" || len(s.queue) != 1 {
		t.Fatalf("offers %v %v, %d queued", ids, classes, len(s.queue))
	}
	start := time.Now()
	if ids, _ := nextClasses(t, s, 2, "0"); len(ids) != 0 || time.Since(start) > 200*time.Millisecond {
		t.Fatalf("the hung run again: %v after %v", ids, time.Since(start))
	}
}

// P2-1: the results of a lookup stay between `next` calls. A run whose
// calls each take 40 % of the budget needs five of them (the token, the
// run, two memberships): it ends over two calls. Before, each `next` began
// again with the run, and the run never ended.
func TestARunOfSlowCallsEndsOverTwoNextCalls(t *testing.T) {
	defer func(d time.Duration) { nextBudget = d }(nextBudget)
	nextBudget = time.Second
	f, ts := newFakeGitHub(t)
	f.members["bob"] = true
	f.run("5", runObject(user("alice", 1), user("bob", 2), 1))
	st := newStub()
	s := lookupSidecar(t, st, ts.URL)
	waitPoll(t, st)
	f.setDelay(400 * time.Millisecond)
	st.put <- jobsOf(1, 5)
	var got []string
	calls := 0
	for ; calls < 5 && len(got) == 0; calls++ {
		_, got = nextClasses(t, s, 1, map[bool]string{true: "5", false: "0"}[calls == 0])
	}
	if len(got) != 1 || got[0] != "member" || calls > 2 {
		t.Fatalf("%v after %d calls", got, calls)
	}
}

// P2-2: a 5xx, a 429, a 403 of a rate limit, a 401 and a failed token are
// no final answer: the job stays queued and gets its real class later; its
// run is not cancelled. Only a final answer (a 404, a 403 of a missing
// permission) gives unknown.
func TestAnAnswerThatIsNotFinalKeepsTheJobQueued(t *testing.T) {
	defer func(d time.Duration) { retryFirst = d }(retryFirst)
	retryFirst = 0
	for _, c := range []struct {
		name, path string
		r          reply
	}{
		{"502 of the run", "5", reply{code: 502}},
		{"500 of the run", "5", reply{code: 500}},
		{"503 of the run", "5", reply{code: 503}},
		{"429 of the run", "5", reply{code: 429, header: map[string]string{"Retry-After": "1"}}},
		{"403 of a secondary limit", "5", reply{code: 403, header: map[string]string{"Retry-After": "60"}}},
		{"403 of the hourly limit", "5", reply{code: 403, header: map[string]string{"X-RateLimit-Remaining": "0"}}},
		{"401 of an old token", "5", reply{code: 401}},
		{"502 of the membership", "alice", reply{code: 502}},
		{"500 of the token", "access_tokens", reply{code: 500}},
	} {
		f, ts := newFakeGitHub(t)
		f.run("5", runObject(user("alice", 1), "", 1))
		st := newStub()
		s := lookupSidecar(t, st, ts.URL)
		// A token after the scope: the job asks for a new one.
		s.trust.token = ""
		f.answer(c.path, c.r)
		waitPoll(t, st)
		st.put <- jobsOf(1, 5)
		ids, classes := nextClasses(t, s, 1, "5")
		withdrawUnknown(t, s, ids, classes)
		if len(ids) != 0 || len(s.queue) != 1 {
			t.Errorf("%s: offered %v %v at once", c.name, ids, classes)
			continue
		}
		ids, classes = nextClasses(t, s, 1, "0")
		withdrawUnknown(t, s, ids, classes)
		if len(classes) != 1 || classes[0] != "member" || f.cancelCount() != 0 {
			t.Errorf("%s: then %v, %d cancels", c.name, classes, f.cancelCount())
		}
	}
	// Final answers: no run, no permission for the membership.
	for _, c := range []struct{ name, path string }{{"404 of the run", "5"}, {"403 of the membership", "alice"}} {
		f, ts := newFakeGitHub(t)
		f.run("5", runObject(user("alice", 1), "", 1))
		st := newStub()
		s := lookupSidecar(t, st, ts.URL)
		f.answer(c.path, reply{code: map[string]int{"5": 404, "alice": 403}[c.path]})
		waitPoll(t, st)
		st.put <- jobsOf(1, 5)
		if _, classes := nextClasses(t, s, 1, "5"); len(classes) != 1 || classes[0] != "unknown" {
			t.Errorf("%s: %v", c.name, classes)
		}
	}
}

// P2-3: a spent budget is no answer about the job. The job waits for the
// next hour of the budget, its run is not cancelled, and the sidecar says
// once that the budget is spent.
func TestASpentBudgetKeepsTheJobQueued(t *testing.T) {
	defer func(d time.Duration) { retryFirst = d }(retryFirst)
	retryFirst = 0
	f, ts := newFakeGitHub(t)
	f.run("5", runObject(user("alice", 1), "", 1))
	st := newStub()
	s := lookupSidecar(t, st, ts.URL)
	// The token and the run: the membership is past the budget.
	s.trust.budget = 2
	waitPoll(t, st)
	st.put <- jobsOf(1, 5, 5)
	var ids, classes []string
	log := stderrOf(t, func() {
		for i := 0; i < 3; i++ {
			ids, classes = nextClasses(t, s, 2, map[bool]string{true: "5", false: "0"}[i == 0])
			withdrawUnknown(t, s, ids, classes)
		}
	})
	if len(ids) != 0 || len(s.queue) != 2 || f.cancelCount() != 0 {
		t.Fatalf("offered %v %v, %d queued, %d cancels", ids, classes, len(s.queue), f.cancelCount())
	}
	if strings.Count(log, "budget") != 1 || !strings.Contains(log, "the trust budget of the hour is spent") {
		t.Fatalf("the log: %q", log)
	}
	// The next hour of the budget.
	s.trust.hourStart = time.Now().Add(-time.Hour)
	if _, classes = nextClasses(t, s, 2, "0"); len(classes) != 2 || classes[0] != "member" {
		t.Fatalf("after the hour: %v", classes)
	}
}

// P2-1: a job with no final answer is not kept for ever. Past the limit of
// attempts, or of its age, it is offered with unknown inputs: a trusted
// set refuses it and its run is cancelled (Q7). The sidecar names the job
// and the cause on stderr, and counts it.
func TestAJobPastTheLimitIsOfferedUnknownAndReported(t *testing.T) {
	defer func(d time.Duration, n int, p time.Duration) { retryFirst, attemptMax, pendingMax = d, n, p }(retryFirst, attemptMax, pendingMax)
	retryFirst, attemptMax = 0, 3
	f, ts := newFakeGitHub(t)
	f.run("5", runObject(user("alice", 1), "", 1))
	for i := 0; i < 5; i++ {
		f.answer("5", reply{code: 502})
	}
	st := newStub()
	s := lookupSidecar(t, st, ts.URL)
	waitPoll(t, st)
	st.put <- jobsOf(1, 5)
	var seen [][]string
	log := stderrOf(t, func() {
		for i := 0; i < 4; i++ {
			ids, classes := nextClasses(t, s, 1, map[bool]string{true: "5", false: "0"}[i == 0])
			withdrawUnknown(t, s, ids, classes)
			seen = append(seen, classes)
		}
	})
	if len(seen[0])+len(seen[1]) != 0 || len(seen[2]) != 1 || seen[2][0] != "unknown" || f.cancelCount() != 1 {
		t.Fatalf("offers %v, %d cancels", seen, f.cancelCount())
	}
	if !strings.Contains(log, "job 1001 of the run 5 of example-org/app") || !strings.Contains(log, "3 attempts") || !strings.Contains(log, "HTTP 502") {
		t.Fatalf("the log: %q", log)
	}
	if !strings.Contains(log, "given up 1") {
		t.Fatalf("no count in the log: %q", log)
	}
	// The age: a spent budget past pendingMax.
	pendingMax = 100 * time.Millisecond
	g, ts2 := newFakeGitHub(t)
	g.run("6", runObject(user("alice", 1), "", 1))
	st2 := newStub()
	s2 := lookupSidecar(t, st2, ts2.URL)
	s2.trust.budget = 1
	waitPoll(t, st2)
	st2.put <- jobsOf(2, 6)
	log = stderrOf(t, func() {
		if _, c := nextClasses(t, s2, 1, "5"); len(c) != 0 {
			t.Errorf("at once: %v", c)
		}
		time.Sleep(200 * time.Millisecond)
		if _, c := nextClasses(t, s2, 1, "0"); len(c) != 1 || c[0] != "unknown" {
			t.Errorf("past the age: %v", c)
		}
	})
	if !strings.Contains(log, "job 2001 of the run 6") || !strings.Contains(log, "budget") {
		t.Fatalf("the log of the age: %q", log)
	}
}

// P2-3: past the budget, a run read before is still read with its ETag:
// a 304 is free, so the jobs of known runs go on while an outsider's runs
// spent the budget. A new run waits.
func TestAKnownRunIsReadPastTheBudget(t *testing.T) {
	f, ts := newFakeGitHub(t)
	f.etags = true
	f.run("1", runObject(user("alice", 1), "", 1))
	f.run("2", runObject(user("alice", 1), "", 1))
	tr := scopeTrust(ts.URL)
	tr.budget = 10
	if in, ok := tr.lookup("example-org", "app", 1, "push"); !ok || in.actorClass != "member" {
		t.Fatalf("run 1: %+v", in)
	}
	tr.used = tr.budget
	if in, ok := tr.lookup("example-org", "app", 1, "push"); !ok || in.actorClass != "member" || f.notModified != 1 {
		t.Fatalf("run 1 past the budget: %+v, %d 304", in, f.notModified)
	}
	if in, ok := tr.lookup("example-org", "app", 2, "push"); ok || f.runReads != 1 {
		t.Fatalf("run 2 past the budget: %+v, %d reads", in, f.runReads)
	}
}

// Each job of a run with no final answer counts its attempts, also when
// the first job of the run in the same `next` set the retry of the run.
func TestEachJobOfARunCountsItsAttempts(t *testing.T) {
	defer func(d time.Duration, n int) { retryFirst, attemptMax = d, n }(retryFirst, attemptMax)
	retryFirst, attemptMax = 100*time.Millisecond, 2
	f, ts := newFakeGitHub(t)
	f.run("5", runObject(user("alice", 1), "", 1))
	for i := 0; i < 5; i++ {
		f.answer("5", reply{code: 502})
	}
	st := newStub()
	s := lookupSidecar(t, st, ts.URL)
	waitPoll(t, st)
	st.put <- jobsOf(1, 5, 5)
	var got []string
	stderrOf(t, func() {
		for i := 0; i < 3 && len(got) == 0; i++ {
			_, got = nextClasses(t, s, 2, map[bool]string{true: "5", false: "0"}[i == 0])
			time.Sleep(250 * time.Millisecond)
		}
	})
	if len(got) != 2 || got[0] != "unknown" || got[1] != "unknown" || len(s.queue) != 0 {
		t.Fatalf("%v, %d queued", got, len(s.queue))
	}
}
