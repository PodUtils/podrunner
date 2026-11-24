package main

import (
	"bufio"
	"context"
	"fmt"
	"os"
	"regexp"
	"strconv"
	"syscall"
	"testing"
	"time"

	"github.com/actions/scaleset"
)

// lookupSidecar is a sidecar on the stub session with the lookups of the
// fake API and the organization example-org.
func lookupSidecar(t *testing.T, st *stubSession, api string) *sidecar {
	ctx, cancel := context.WithCancel(context.Background())
	t.Cleanup(cancel)
	p := newPoller(st)
	go p.run(ctx)
	tr := scopeTrust(api)
	tr.http.Timeout = lookupTimeout
	if err := tr.scope("https://github.com/example-org", false); err != nil {
		t.Fatal(err)
	}
	return &sidecar{o: options{source: "s"}, ctx: ctx, poll: p, trust: tr, jobs: map[string]*job{}, byJobID: map[string]string{}}
}

// jobsOf is one message with a JobAvailable for each run of runs.
func jobsOf(id int, runs ...int64) *scaleset.RunnerScaleSetMessage {
	m := &scaleset.RunnerScaleSetMessage{MessageID: id}
	for i, run := range runs {
		req := int64(id*1000 + i + 1)
		m.JobAvailableMessages = append(m.JobAvailableMessages, &scaleset.JobAvailable{JobMessageBase: scaleset.JobMessageBase{
			RunnerRequestID: req, JobID: "g-" + strconv.FormatInt(req, 10), OwnerName: "example-org", RepositoryName: "app",
			WorkflowRunID: run, EventName: "push",
		}})
	}
	return m
}

// offerClasses gives the actor class of each offer of a reply.
func offerClasses(t *testing.T, rep map[string]any) []string {
	t.Helper()
	ev, ok := rep["events"].([]any)
	if !ok {
		t.Fatalf("no events: %v", rep)
	}
	var out []string
	for _, e := range ev {
		if m := e.(map[string]any); m["event"] == "offer" {
			in := m["offer"].(map[string]any)["trust_inputs"].(map[string]any)
			out = append(out, in["actor_class"].(string))
		}
	}
	return out
}

// A of the second T-504 review: a matrix of 200 jobs of one run, with 100
// ms for each answer of GitHub. The run is read once for each `next`, and
// a lookup is made only for a job that the capacity takes: each `next`
// answers far inside the reply limit of the mint (15 s), and each job is
// offered over the next calls. Before, `next` read the run for each job
// of the message (20 s): podrunner killed the sidecar, and the deleted
// messages were lost.
func TestAMatrixOfOneRunAnswersInsideTheReplyLimit(t *testing.T) {
	f, ts := newFakeGitHub(t)
	f.run("5", runObject(user("alice", 1), user("alice", 1), 1))
	f.setDelay(100 * time.Millisecond)
	st := newStub()
	s := lookupSidecar(t, st, ts.URL)
	waitPoll(t, st)
	runs := make([]int64, 200)
	for i := range runs {
		runs[i] = 5
	}
	st.put <- jobsOf(1, runs...)
	offered := 0
	for call := 0; offered < 200 && call < 20; call++ {
		// The first call waits for the message; the engine then calls with
		// no wait.
		wait := map[bool]string{true: "5", false: "0"}[call == 0]
		start := time.Now()
		rep := s.handle([]byte(`{"schema":1,"op":"next","capacity":50,"wait_s":` + wait + `}`))
		if took := time.Since(start); took > 3*time.Second {
			t.Fatalf("next %d took %v", call, took)
		}
		for _, c := range offerClasses(t, rep) {
			if c != "member" {
				t.Fatalf("next %d: a class %s", call, c)
			}
			offered++
		}
	}
	if offered != 200 || s.TrustLookupFailed != 0 {
		t.Fatalf("%d offered, %d failed lookups", offered, s.TrustLookupFailed)
	}
	if f.runReads > 4 {
		t.Errorf("%d reads of the run for 4 calls", f.runReads)
	}
}

// B of the second T-504 review: the budget of the hour counts each call
// of GitHub (the token, the run, each membership), not each lookup.
func TestTheTrustBudgetCountsEachCall(t *testing.T) {
	f, ts := newFakeGitHub(t)
	f.run("1", runObject(user("alice", 1), "", 1))
	f.run("2", runObject(user("alice", 1), "", 1))
	tr := scopeTrust(ts.URL)
	tr.budget = 3
	if in, ok := tr.lookup("example-org", "app", 1, "push"); !ok || in.actorClass != "member" {
		t.Fatalf("run 1: %+v %v", in, ok)
	}
	// The budget is spent: a lookup is not made, and the job is denied.
	if in, ok := tr.lookup("example-org", "app", 2, "push"); ok || in.actorClass != "unknown" || tr.Throttled == 0 {
		t.Fatalf("run 2: %+v %v, %d throttled", in, ok, tr.Throttled)
	}
}

// B: a run that did not change is asked for with its ETag; GitHub answers
// 304, which does not count against its rate limit or the budget.
func TestAnUnchangedRunIsAConditionalRead(t *testing.T) {
	f, ts := newFakeGitHub(t)
	f.etags = true
	f.run("1", runObject(user("alice", 1), "", 1))
	tr := scopeTrust(ts.URL)
	// The token, the run and the membership: 3. A conditional read needs
	// room for one call, and gives it back at a 304.
	tr.budget = 4
	for i := 0; i < 5; i++ {
		if in, ok := tr.lookup("example-org", "app", 1, "push"); !ok || in.actorClass != "member" {
			t.Fatalf("lookup %d: %+v %v", i, in, ok)
		}
	}
	if f.runReads != 1 || f.notModified != 4 || f.memberLookups != 1 || tr.Throttled != 0 || tr.used != 3 {
		t.Fatalf("%d reads, %d 304, %d memberships, %d throttled, %d used", f.runReads, f.notModified, f.memberLookups, tr.Throttled, tr.used)
	}
	// A new attempt changes the object: a full read, and its classes.
	f.run("1", runObject(user("alice", 1), user("mallory", 3), 2))
	tr.budget = 0
	if in, _ := tr.lookup("example-org", "app", 1, "push"); in.actorClass != "outside" || f.runReads != 2 {
		t.Fatalf("attempt 2: %+v, %d reads", in, f.runReads)
	}
}

// A: the lookups of one `next` have a time budget. The jobs that it does
// not reach stay queued in their order, and a later `next` offers them;
// none is refused and none is lost.
func TestTheLookupsOfANextHaveATimeBudget(t *testing.T) {
	defer func(d time.Duration) { nextBudget = d }(nextBudget)
	nextBudget = time.Second
	f, ts := newFakeGitHub(t)
	runs := make([]int64, 30)
	for i := range runs {
		runs[i] = int64(100 + i)
		f.run(strconv.Itoa(100+i), runObject(user("alice", 1), "", 1))
	}
	f.setDelay(100 * time.Millisecond)
	st := newStub()
	s := lookupSidecar(t, st, ts.URL)
	waitPoll(t, st)
	st.put <- jobsOf(1, runs...)
	var ids []string
	for call := 0; len(ids) < 30 && call < 20; call++ {
		wait := map[bool]string{true: "5", false: "0"}[call == 0]
		start := time.Now()
		rep := s.handle([]byte(`{"schema":1,"op":"next","capacity":30,"wait_s":` + wait + `}`))
		took := time.Since(start)
		got := offers(rep)
		if call == 0 && len(got) >= 30 {
			t.Fatalf("the budget took no job: %d offers", len(got))
		}
		if took > nextBudget+500*time.Millisecond {
			t.Fatalf("next %d took %v", call, took)
		}
		for _, c := range offerClasses(t, rep) {
			if c != "member" {
				t.Fatalf("next %d: a class %s", call, c)
			}
		}
		ids = append(ids, got...)
	}
	for i, id := range ids {
		if want := strconv.Itoa(1000 + i + 1); id != want {
			t.Fatalf("offer %d is %s, want %s: %v", i, id, want, ids)
		}
	}
	if len(ids) != 30 || s.TrustLookupFailed != 0 {
		t.Fatalf("%d offers, %d failed lookups", len(ids), s.TrustLookupFailed)
	}
}

// A: a lookup that GitHub does not answer in the budget leaves its job
// queued; the job is offered when GitHub answers.
func TestAHungLookupLeavesItsJobQueued(t *testing.T) {
	defer func(d time.Duration) { nextBudget = d }(nextBudget)
	nextBudget = 300 * time.Millisecond
	f, ts := newFakeGitHub(t)
	f.run("7", runObject(user("alice", 1), "", 1))
	st := newStub()
	s := lookupSidecar(t, st, ts.URL)
	waitPoll(t, st)
	f.setDelay(2 * time.Second)
	st.put <- jobsOf(1, 7, 7)
	start := time.Now()
	rep := s.handle([]byte(`{"schema":1,"op":"next","capacity":2,"wait_s":5}`))
	if got := offers(rep); len(got) != 0 || time.Since(start) > time.Second || len(s.queue) != 2 || s.TrustLookupFailed != 0 {
		t.Fatalf("%v after %v, %d queued, %d failed", got, time.Since(start), len(s.queue), s.TrustLookupFailed)
	}
	f.setDelay(0)
	rep = s.handle([]byte(`{"schema":1,"op":"next","capacity":2,"wait_s":0}`))
	if got := offerClasses(t, rep); len(got) != 2 || got[0] != "member" || got[1] != "member" {
		t.Fatalf("%v", rep)
	}
}

// The time of the lookups of one `next`, and of one call, stay inside the
// reply limit of the mint, with room for the rest of the call. A real
// `next` is timed with the limits of the sidecar: against a sign module
// that answers after the budget, and against a GitHub that never answers
// (P3-8 of the third T-504 review: the read of the sign channel had no
// time limit, and `next` waited for sign).
func TestTheBudgetOfANextIsInsideTheReplyLimit(t *testing.T) {
	src, err := os.ReadFile("../../crates/podrunner-mint/src/sidecar.rs")
	if err != nil {
		t.Fatal(err)
	}
	m := regexp.MustCompile(`const REPLY_LIMIT: Duration = Duration::from_secs\((\d+)\)`).FindSubmatch(src)
	if m == nil {
		t.Fatal("no REPLY_LIMIT")
	}
	n, _ := strconv.Atoi(string(m[1]))
	limit := time.Duration(n) * time.Second
	if nextBudget+3*time.Second > limit || lookupTimeout > nextBudget {
		t.Fatalf("the budget %v and the call limit %v against the reply limit %v", nextBudget, lookupTimeout, limit)
	}
	for _, c := range []struct {
		name string
		sign time.Duration
		gh   time.Duration
	}{{"a slow sign", nextBudget + 20*time.Second, 0}, {"a hung GitHub", 100 * time.Millisecond, time.Minute}} {
		f, ts := newFakeGitHub(t)
		f.run("5", runObject(user("alice", 1), "", 1))
		st := newStub()
		s := lookupSidecar(t, st, ts.URL)
		f.setDelay(c.gh)
		// No installation token from the scope: the lookup asks sign.
		s.trust.signer, s.trust.token = newSigner(slowSign(t, c.sign), "1"), ""
		waitPoll(t, st)
		st.put <- jobsOf(1, 5)
		done := make(chan time.Duration, 1)
		go func() {
			start := time.Now()
			s.handle([]byte(`{"schema":1,"op":"next","capacity":1,"wait_s":5}`))
			done <- time.Since(start)
		}()
		select {
		case took := <-done:
			if took > nextBudget+time.Second {
				t.Errorf("%s: next took %v", c.name, took)
			}
		case <-time.After(limit - 3*time.Second):
			t.Fatalf("%s: next did not answer in %v", c.name, limit-3*time.Second)
		}
	}
}

// slowSign is the descriptor 3 of a sign module that answers each request
// after d.
func slowSign(t *testing.T, d time.Duration) *os.File {
	fds, err := syscall.Socketpair(syscall.AF_UNIX, syscall.SOCK_STREAM, 0)
	if err != nil {
		t.Fatal(err)
	}
	ours, theirs := os.NewFile(uintptr(fds[0]), "ours"), os.NewFile(uintptr(fds[1]), "theirs")
	t.Cleanup(func() { ours.Close(); theirs.Close() })
	go func() {
		r := bufio.NewReader(theirs)
		for {
			if _, err := r.ReadBytes('\n'); err != nil {
				return
			}
			time.Sleep(d)
			fmt.Fprintf(theirs, "{\"token\":\"a.b.c\",\"exp\":%d}\n", time.Now().Unix()+500)
		}
	}()
	return ours
}
