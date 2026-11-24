package main

import (
	"net/http"
	"net/http/httptest"
	"strings"
	"sync"
	"testing"

	"github.com/actions/scaleset"
)

// T-169: a job that podrunner refuses gets its run cancelled, once for the
// run; a wait that ran out cancels nothing.
func TestARefusedJobCancelsItsRunOnce(t *testing.T) {
	var mu sync.Mutex
	var posts []string
	ts := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch {
		case strings.HasSuffix(r.URL.Path, "/access_tokens"):
			w.WriteHeader(http.StatusCreated)
			_, _ = w.Write([]byte(`{"token":"t","expires_at":"2099-01-01T00:00:00Z"}`))
		case r.Method == http.MethodPost:
			mu.Lock()
			posts = append(posts, r.URL.Path)
			mu.Unlock()
			w.WriteHeader(http.StatusAccepted)
		default:
			w.WriteHeader(http.StatusNotFound)
		}
	}))
	defer ts.Close()
	st := newStub()
	s := stubSidecar(t, st)
	s.trust = &trust{api: ts.URL, installation: 7, signer: &signer{cached: "a.b.c", until: farFuture}}
	waitPoll(t, st)
	job := func(id string, run int64) *scaleset.JobAssigned {
		return &scaleset.JobAssigned{JobMessageBase: scaleset.JobMessageBase{JobID: id, OwnerName: "o", RepositoryName: "r", WorkflowRunID: run}}
	}
	st.put <- &scaleset.RunnerScaleSetMessage{MessageID: 1, JobAssignedMessages: []*scaleset.JobAssigned{job("g-1", 5), job("g-2", 5), job("g-3", 6)}}
	s.handle([]byte(`{"schema":1,"op":"next","capacity":3,"wait_s":5}`))
	for _, w := range []string{
		`{"schema":1,"op":"withdraw","job_id":"a-g-1","reason":"tier-mismatch"}`,
		`{"schema":1,"op":"withdraw","job_id":"a-g-2","reason":"tier-mismatch"}`,
		`{"schema":1,"op":"withdraw","job_id":"a-g-3","reason":"wait-deadline"}`,
	} {
		if r := s.handle([]byte(w)); r["reply"] != "done" {
			t.Fatalf("%s: %v", w, r)
		}
	}
	mu.Lock()
	defer mu.Unlock()
	if len(posts) != 1 || posts[0] != "/repos/o/r/actions/runs/5/cancel" {
		t.Fatalf("posts %v", posts)
	}
}
