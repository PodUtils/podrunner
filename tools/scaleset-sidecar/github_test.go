package main

import (
	"net/http"
	"net/http/httptest"
	"strconv"
	"strings"
	"sync"
	"testing"
	"time"
)

// fakeGitHub is a REST API for the scope and the trust lookups. An owner
// in owners has its type ("User" or "Organization"); each other owner does
// not exist (404). A run is its JSON object, which a test can change.
type fakeGitHub struct {
	mu      sync.Mutex
	owners  map[string]string
	ids     map[string]int
	members map[string]bool
	runs    map[string]string
	// status: when it is not 0, the answer of /users and /orgs.
	status int
	// The lookups of membership and of the owner.
	memberLookups, ownerLookups int
	// delay: the time of each answer. etags: a run has an ETag, and an
	// If-None-Match of it gets 304.
	delay time.Duration
	etags bool
	// The reads of a run (200), its 304 answers, and each request.
	runReads, notModified, requests int
	// slow: the time of each answer for one run id. script: the answers
	// of a path (its last part: a run id, a login, "access_tokens"), used
	// once each in order before the normal answer.
	slow   map[string]time.Duration
	script map[string][]reply
	// The POSTs that cancel a run.
	cancels int
}

// reply is one scripted answer: a status and its headers.
type reply struct {
	code   int
	header map[string]string
}

// setDelay sets the time of each answer under the lock: the handler reads
// it in another goroutine.
func (f *fakeGitHub) setDelay(d time.Duration) {
	f.mu.Lock()
	defer f.mu.Unlock()
	f.delay = d
}

// answer scripts the next answers of a path.
func (f *fakeGitHub) answer(last string, rs ...reply) {
	f.mu.Lock()
	defer f.mu.Unlock()
	f.script[last] = append(f.script[last], rs...)
}

// slowRun sets the time of each answer for one run.
func (f *fakeGitHub) slowRun(id string, d time.Duration) {
	f.mu.Lock()
	defer f.mu.Unlock()
	f.slow[id] = d
}

func (f *fakeGitHub) cancelCount() int {
	f.mu.Lock()
	defer f.mu.Unlock()
	return f.cancels
}

func newFakeGitHub(t *testing.T) (*fakeGitHub, *httptest.Server) {
	f := &fakeGitHub{
		owners:  map[string]string{"example-org": "Organization", "alice": "User", "kate": "User", "mallory": "User"},
		ids:     map[string]int{"example-org": 100, "alice": 1, "kate": 2, "mallory": 3},
		members: map[string]bool{"alice": true},
		runs:    map[string]string{},
		slow:    map[string]time.Duration{},
		script:  map[string][]reply{},
	}
	ts := httptest.NewServer(http.HandlerFunc(f.serve))
	t.Cleanup(ts.Close)
	return f, ts
}

// run sets the run object of the run id.
func (f *fakeGitHub) run(id, object string) {
	f.mu.Lock()
	defer f.mu.Unlock()
	f.runs[id] = object
}

func (f *fakeGitHub) serve(w http.ResponseWriter, r *http.Request) {
	p := r.URL.Path
	last := p[strings.LastIndex(p, "/")+1:]
	f.mu.Lock()
	delay := f.delay
	if strings.Contains(p, "/actions/runs/") && f.slow[last] != 0 {
		delay = f.slow[last]
	}
	f.mu.Unlock()
	select {
	case <-time.After(delay):
	case <-r.Context().Done():
		return
	}
	f.mu.Lock()
	defer f.mu.Unlock()
	f.requests++
	if r.Method == http.MethodPost && last == "cancel" {
		f.cancels++
		w.WriteHeader(http.StatusAccepted)
		return
	}
	if rs := f.script[last]; len(rs) > 0 {
		f.script[last] = rs[1:]
		for k, v := range rs[0].header {
			w.Header().Set(k, v)
		}
		w.WriteHeader(rs[0].code)
		return
	}
	owner := func() {
		f.ownerLookups++
		typ, ok := f.owners[strings.ToLower(last)]
		switch {
		case f.status != 0:
			w.WriteHeader(f.status)
		case !ok || (strings.HasPrefix(p, "/orgs/") && typ != "Organization"):
			w.WriteHeader(http.StatusNotFound)
		default:
			_, _ = w.Write([]byte(`{"login":"` + last + `","id":` + strconv.Itoa(f.ids[strings.ToLower(last)]) + `,"type":"` + typ + `"}`))
		}
	}
	switch {
	case strings.HasSuffix(p, "/access_tokens"):
		w.WriteHeader(http.StatusCreated)
		_, _ = w.Write([]byte(`{"token":"t","expires_at":"2099-01-01T00:00:00Z"}`))
	case strings.Contains(p, "/members/"):
		f.memberLookups++
		if f.members[last] {
			w.WriteHeader(http.StatusNoContent)
		} else {
			w.WriteHeader(http.StatusNotFound)
		}
	case strings.Contains(p, "/actions/runs/"):
		if v, ok := f.runs[last]; ok {
			tag := `"` + strconv.Itoa(len(v)) + "-" + strconv.Itoa(int(crc(v))) + `"`
			if f.etags && r.Header.Get("If-None-Match") == tag {
				f.notModified++
				w.WriteHeader(http.StatusNotModified)
				return
			}
			if f.etags {
				w.Header().Set("ETag", tag)
			}
			f.runReads++
			_, _ = w.Write([]byte(v))
		} else {
			w.WriteHeader(http.StatusNotFound)
		}
	case strings.HasPrefix(p, "/users/"), strings.HasPrefix(p, "/orgs/"):
		owner()
	default:
		w.WriteHeader(http.StatusNotFound)
	}
}

// runObject is a private push run of one repository, with its actor, its
// triggering actor ("" for no field) and its attempt (0 for no field).
func runObject(actor, triggering string, attempt int) string {
	v := `{"event":"push","actor":` + actor
	if triggering != "" {
		v += `,"triggering_actor":` + triggering
	}
	if attempt != 0 {
		v += `,"run_attempt":` + strconv.Itoa(attempt)
	}
	return v + `,"repository":{"id":1,"private":true},"head_repository":{"id":1}}`
}

// user is the actor object of a login, with its id when id is not 0.
func user(login string, id int) string {
	if id == 0 {
		return `{"login":"` + login + `"}`
	}
	return `{"login":"` + login + `","id":` + strconv.Itoa(id) + `}`
}

// crc is a small sum of a run object for its ETag.
func crc(s string) uint32 {
	var h uint32 = 2166136261
	for i := 0; i < len(s); i++ {
		h = (h ^ uint32(s[i])) * 16777619
	}
	return h
}
