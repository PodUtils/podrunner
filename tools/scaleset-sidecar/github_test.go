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
}

func newFakeGitHub(t *testing.T) (*fakeGitHub, *httptest.Server) {
	f := &fakeGitHub{
		owners:  map[string]string{"example-org": "Organization", "alice": "User", "kate": "User", "mallory": "User"},
		ids:     map[string]int{"example-org": 100, "alice": 1, "kate": 2, "mallory": 3},
		members: map[string]bool{"alice": true},
		runs:    map[string]string{},
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
	f.mu.Lock()
	delay := f.delay
	f.mu.Unlock()
	time.Sleep(delay)
	f.mu.Lock()
	defer f.mu.Unlock()
	f.requests++
	p := r.URL.Path
	last := p[strings.LastIndex(p, "/")+1:]
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
