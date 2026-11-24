package main

import (
	"context"
	"errors"
	"sync"
	"testing"
	"time"

	"github.com/actions/scaleset"
)

// stubSession holds each poll until a message is put or the poll is
// cancelled, as the long poll of GitHub does.
type stubSession struct {
	mu        sync.Mutex
	put       chan *scaleset.RunnerScaleSetMessage
	err       error
	deleteErr error
	deleted   []int
	// The capacity of each poll, in order.
	capacities []int
	polls      chan int
}

func newStub() *stubSession {
	return &stubSession{put: make(chan *scaleset.RunnerScaleSetMessage, 4), polls: make(chan int, 64)}
}

func (s *stubSession) GetMessage(ctx context.Context, _ int, capacity int) (*scaleset.RunnerScaleSetMessage, error) {
	s.mu.Lock()
	s.capacities = append(s.capacities, capacity)
	err := s.err
	s.mu.Unlock()
	s.polls <- capacity
	if err != nil {
		return nil, err
	}
	select {
	case m := <-s.put:
		return m, nil
	case <-ctx.Done():
		return nil, ctx.Err()
	}
}

func (s *stubSession) DeleteMessage(_ context.Context, id int) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.deleteErr != nil {
		err := s.deleteErr
		s.deleteErr = nil
		return err
	}
	s.deleted = append(s.deleted, id)
	return nil
}

func stubSidecar(t *testing.T, st *stubSession) *sidecar {
	ctx, cancel := context.WithCancel(context.Background())
	t.Cleanup(cancel)
	p := newPoller(st)
	go p.run(ctx)
	// A run that the API does not know is a final answer: unknown inputs.
	_, ts := newFakeGitHub(t)
	tr := scopeTrust(ts.URL)
	return &sidecar{o: options{source: "s"}, ctx: ctx, poll: p, trust: tr, jobs: map[string]*job{}, byJobID: map[string]string{}}
}

func waitPoll(t *testing.T, st *stubSession) int {
	select {
	case c := <-st.polls:
		return c
	case <-time.After(5 * time.Second):
		t.Fatal("no poll")
		return 0
	}
}

// T-164: the long poll of GitHub holds; `next` answers in `wait_s` with
// no events, inside the reply limit of the mint.
func TestNextAnswersInWaitWhileTheLongPollHolds(t *testing.T) {
	st := newStub()
	s := stubSidecar(t, st)
	start := time.Now()
	rep := s.handle([]byte(`{"schema":1,"op":"next","capacity":1,"wait_s":1}`))
	took := time.Since(start)
	if rep["reply"] != "events" || len(rep["events"].([]any)) != 0 {
		t.Fatalf("%v", rep)
	}
	if took < 900*time.Millisecond || took > 3*time.Second {
		t.Fatalf("next took %v, want about 1 s", took)
	}
	start = time.Now()
	rep = s.handle([]byte(`{"schema":1,"op":"next","capacity":1,"wait_s":0}`))
	if time.Since(start) > 500*time.Millisecond || rep["reply"] != "events" {
		t.Fatalf("wait_s 0 took %v: %v", time.Since(start), rep)
	}
}

// A message that the poll finds is given by the next `next`, once, after
// its delete.
func TestAFoundMessageIsGivenOnce(t *testing.T) {
	st := newStub()
	s := stubSidecar(t, st)
	waitPoll(t, st)
	st.put <- &scaleset.RunnerScaleSetMessage{MessageID: 7, JobStartedMessages: []*scaleset.JobStarted{{RunnerName: "r"}}}
	rep := s.handle([]byte(`{"schema":1,"op":"next","capacity":1,"wait_s":5}`))
	if ev := rep["events"].([]any); len(ev) != 1 || ev[0].(map[string]any)["event"] != "started" {
		t.Fatalf("%v", rep)
	}
	rep = s.handle([]byte(`{"schema":1,"op":"next","capacity":1,"wait_s":0}`))
	if len(rep["events"].([]any)) != 0 {
		t.Fatalf("given twice: %v", rep)
	}
	st.mu.Lock()
	defer st.mu.Unlock()
	if len(st.deleted) != 1 || st.deleted[0] != 7 {
		t.Fatalf("deleted %v", st.deleted)
	}
}

// A new capacity cancels the poll in flight and polls with it at once.
func TestANewCapacityStartsANewPoll(t *testing.T) {
	st := newStub()
	s := stubSidecar(t, st)
	if c := waitPoll(t, st); c != 0 {
		t.Fatalf("first poll with capacity %d", c)
	}
	s.handle([]byte(`{"schema":1,"op":"next","capacity":2,"wait_s":0}`))
	if c := waitPoll(t, st); c != 2 {
		t.Fatalf("the next poll has capacity %d, want 2", c)
	}
}

// A message with a failed delete is not given: it comes again.
func TestAMessageWithAFailedDeleteIsNotGiven(t *testing.T) {
	st := newStub()
	st.deleteErr = errors.New("delete failed")
	s := stubSidecar(t, st)
	waitPoll(t, st)
	st.put <- &scaleset.RunnerScaleSetMessage{MessageID: 9}
	waitPoll(t, st)
	p := s.poll
	p.mu.Lock()
	n, last := len(p.pending), p.lastID
	p.mu.Unlock()
	if n != 0 || last != 0 {
		t.Fatalf("%d pending, last id %d", n, last)
	}
}

// A refusal of the poll is the reply of `next`: 401 is source-revoked.
func TestARefusedPollIsTheReplyOfNext(t *testing.T) {
	st := newStub()
	st.err = errors.New(`request GET https://example.invalid/m failed(status="401 Unauthorized")`)
	s := stubSidecar(t, st)
	waitPoll(t, st)
	rep := s.handle([]byte(`{"schema":1,"op":"next","capacity":1,"wait_s":2}`))
	if rep["code"] != "source-revoked" {
		t.Fatalf("%v", rep)
	}
}
