package main

import (
	"context"
	"testing"
	"time"
)

// The fourth T-504 review: a batch kept the result of a run by its id
// only. The first job of the run used a run object that was read before
// a later job came, and each later job of the run took that result: the
// job of a re-run by an outsider got the class of the first attempt
// (member). A job is classed only by a run object whose read began after
// the job came.
func TestALaterJobOfARunIsNotClassedByAnOlderRead(t *testing.T) {
	f, ts := newFakeGitHub(t)
	f.etags = true
	f.run("5", runObject(user("alice", 1), user("alice", 1), 1))
	tr := scopeTrust(ts.URL)
	sinceA := time.Now()
	if _, _, st, _ := tr.lookupIn(newBatch(context.Background()), "example-org", "app", 5, "push", sinceA); st != final {
		t.Fatalf("the first read: %v", st)
	}
	// An outsider re-runs the run; a job of the new attempt comes.
	f.run("5", runObject(user("alice", 1), user("mallory", 3), 2))
	sinceB := time.Now()
	b := newBatch(context.Background())
	if in, _, _, _ := tr.lookupIn(b, "example-org", "app", 5, "push", sinceA); in.actorClass != "member" {
		t.Fatalf("job A of attempt 1: %+v", in)
	}
	if in, _, _, _ := tr.lookupIn(b, "example-org", "app", 5, "push", sinceB); in.actorClass != "outside" {
		t.Fatalf("job B of attempt 2: %+v, %d reads", in, f.runReads)
	}
	// The read for B serves each job that came before it, with no call.
	reads := f.runReads + f.notModified
	if in, _, _, _ := tr.lookupIn(b, "example-org", "app", 5, "push", sinceA); in.actorClass != "outside" || f.runReads+f.notModified != reads {
		t.Fatalf("job A after the read for B: %+v, %d calls", in, f.runReads+f.notModified-reads)
	}
	// A job that came after both reads: a conditional read (304).
	sinceC := time.Now()
	if in, _, _, _ := tr.lookupIn(b, "example-org", "app", 5, "push", sinceC); in.actorClass != "outside" || f.notModified != 1 {
		t.Fatalf("job C: %+v, %d 304", in, f.notModified)
	}
}

// The same over two `next` calls: job A of attempt 1 has no final answer
// after its run was read (a 502 of the membership). An outsider re-runs
// the run, and job B of attempt 2 comes. In the next `next`, A comes first
// and uses the kept object of attempt 1; B must not take its result.
func TestAJobOfAReRunAfterAKeptReadIsNotAMember(t *testing.T) {
	defer func(d time.Duration) { retryFirst = d }(retryFirst)
	retryFirst = 0
	f, ts := newFakeGitHub(t)
	f.run("5", runObject(user("alice", 1), user("alice", 1), 1))
	f.answer("alice", reply{code: 502})
	st := newStub()
	s := lookupSidecar(t, st, ts.URL)
	waitPoll(t, st)
	st.put <- jobsOf(1, 5)
	if ids, _ := nextClasses(t, s, 2, "5"); len(ids) != 0 || len(s.queue) != 1 || f.runReads != 1 {
		t.Fatalf("job A: offered %v, %d queued, %d reads", ids, len(s.queue), f.runReads)
	}
	f.run("5", runObject(user("alice", 1), user("mallory", 3), 2))
	waitPoll(t, st)
	st.put <- jobsOf(2, 5)
	ids, classes := nextClasses(t, s, 2, "5")
	withdrawUnknown(t, s, ids, classes)
	if len(ids) != 2 || ids[1] != "2001" || classes[1] != "outside" || f.runReads != 2 {
		t.Fatalf("offers %v %v, %d reads", ids, classes, f.runReads)
	}
}
