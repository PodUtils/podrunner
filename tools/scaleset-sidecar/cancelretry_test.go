package main

import (
	"fmt"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync"
	"testing"
	"time"
)

// cancelAPI is a fake GitHub for the cancel endpoint: it answers the
// calls in turn with `codes` (the last one repeats), and it gives a new
// installation token for each request of one.
type cancelAPI struct {
	mu     sync.Mutex
	codes  []int
	body   string
	delay  time.Duration
	posts  int
	tokens int
	auths  []string
	srv    *httptest.Server
}

func newCancelAPI(t *testing.T, codes ...int) *cancelAPI {
	a := &cancelAPI{codes: codes}
	a.srv = httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		a.mu.Lock()
		if strings.HasSuffix(r.URL.Path, "/access_tokens") {
			a.tokens++
			n := a.tokens
			a.mu.Unlock()
			w.WriteHeader(http.StatusCreated)
			fmt.Fprintf(w, `{"token":"t%d","expires_at":"2099-01-01T00:00:00Z"}`, n)
			return
		}
		if r.Method != http.MethodPost || !strings.HasSuffix(r.URL.Path, "/cancel") {
			a.mu.Unlock()
			w.WriteHeader(http.StatusNotFound)
			return
		}
		code := a.codes[min(a.posts, len(a.codes)-1)]
		a.posts++
		a.auths = append(a.auths, r.Header.Get("Authorization"))
		delay, body := a.delay, a.body
		a.mu.Unlock()
		select {
		case <-time.After(delay):
		case <-r.Context().Done():
			return
		}
		w.WriteHeader(code)
		_, _ = w.Write([]byte(body))
	}))
	t.Cleanup(a.srv.Close)
	return a
}

func (a *cancelAPI) count() (posts, tokens int) {
	a.mu.Lock()
	defer a.mu.Unlock()
	return a.posts, a.tokens
}

// fastRetries shortens the waits and the age limit of the cancels.
func fastRetries(t *testing.T, age time.Duration) {
	a, b, c, d := retryFirst, retryMax, cancelMaxAge, cancelListMax
	retryFirst, retryMax, cancelMaxAge = 10*time.Millisecond, 20*time.Millisecond, age
	t.Cleanup(func() { retryFirst, retryMax, cancelMaxAge, cancelListMax = a, b, c, d })
}

func cancelSidecar(t *testing.T, api *cancelAPI) *sidecar {
	s := stubSidecar(t, newStub())
	s.trust = scopeTrust(api.srv.URL)
	return s
}

func nextOnce(s *sidecar) {
	s.handle([]byte(`{"schema":1,"op":"next","capacity":1,"wait_s":0}`))
}

// nextUntil sends `next` until cond holds, at most 3 s.
func nextUntil(t *testing.T, s *sidecar, cond func() bool) {
	t.Helper()
	for end := time.Now().Add(3 * time.Second); time.Now().Before(end); time.Sleep(5 * time.Millisecond) {
		nextOnce(s)
		if cond() {
			return
		}
	}
	t.Fatal("the condition did not hold in 3 s")
}

func refusedJob() *job { return &job{owner: "o", repo: "r", run: 5} }

// T-623: a cancel that GitHub answers 502 is tried again by a later
// `next`, and the run is marked at the 202 only.
func TestACancelThatFailsIsTriedAgain(t *testing.T) {
	fastRetries(t, time.Minute)
	api := newCancelAPI(t, 502, 202)
	s := cancelSidecar(t, api)
	s.cancelRefused(refusedJob(), "tier-mismatch")
	if p, _ := api.count(); p != 1 || s.cancelled[5] {
		t.Fatalf("after the 502: %d posts, cancelled %v", p, s.cancelled[5])
	}
	nextUntil(t, s, func() bool { p, _ := api.count(); return p >= 2 })
	if !s.cancelled[5] || len(s.cancelRetry) != 0 {
		t.Fatalf("after the 202: cancelled %v, list %d", s.cancelled[5], len(s.cancelRetry))
	}
	// Marked once: another refused job of the run, and more `next`, call nothing.
	s.cancelRefused(refusedJob(), "tier-mismatch")
	nextOnce(s)
	time.Sleep(50 * time.Millisecond)
	nextOnce(s)
	if p, _ := api.count(); p != 2 {
		t.Fatalf("%d posts, want 2", p)
	}
}

// T-623: the other answers. A 429 and no answer are tried again; a 409
// that says the run is complete ends the cancel; a 404 is final.
func TestTheAnswersOfTheCancelEndpoint(t *testing.T) {
	cases := []struct {
		name  string
		codes []int
		body  string
		posts int
		done  bool
	}{
		{"429 then 202", []int{429, 202}, "", 2, true},
		{"409 complete", []int{409}, `{"message":"Cannot cancel a workflow run that is completed."}`, 1, true},
		{"409 other", []int{409, 202}, `{"message":"busy"}`, 2, true},
		{"404 is final", []int{404}, "", 1, true},
		{"403 permission is final", []int{403}, `{"message":"Resource not accessible by integration"}`, 1, true},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			fastRetries(t, time.Minute)
			api := newCancelAPI(t, c.codes...)
			api.body = c.body
			s := cancelSidecar(t, api)
			s.cancelRefused(refusedJob(), "tier-mismatch")
			for range 20 {
				nextOnce(s)
				time.Sleep(15 * time.Millisecond)
			}
			if p, _ := api.count(); p != c.posts || s.cancelled[5] != c.done {
				t.Fatalf("%d posts, cancelled %v", p, s.cancelled[5])
			}
		})
	}
}

// T-623: a 401 drops the installation token, so the retry gets a new one.
func TestACancelThatGetsA401TakesANewToken(t *testing.T) {
	fastRetries(t, time.Minute)
	api := newCancelAPI(t, 401, 202)
	s := cancelSidecar(t, api)
	s.cancelRefused(refusedJob(), "tier-mismatch")
	nextUntil(t, s, func() bool { p, _ := api.count(); return p >= 2 })
	api.mu.Lock()
	defer api.mu.Unlock()
	if api.tokens != 2 || api.auths[0] != "Bearer t1" || api.auths[1] != "Bearer t2" || !s.cancelled[5] {
		t.Fatalf("tokens %d, auths %v, cancelled %v", api.tokens, api.auths, s.cancelled[5])
	}
}

// T-623: a PAT that gets a 401 is not tried in a loop (T-624 gives the
// new token): one call, one line, and the run is settled.
func TestACancelThatGetsA401WithAPATIsNotTriedAgain(t *testing.T) {
	fastRetries(t, time.Minute)
	api := newCancelAPI(t, 401)
	s := cancelSidecar(t, api)
	s.trust.pat = true
	s.trust.signer.patToken = "p"
	out := stderrOf(t, func() {
		s.cancelRefused(refusedJob(), "tier-mismatch")
		for range 10 {
			nextOnce(s)
			time.Sleep(15 * time.Millisecond)
		}
	})
	if p, _ := api.count(); p != 1 || strings.Count(out, "cancel the run 5") != 1 || !strings.Contains(out, "not tried again") {
		t.Fatalf("%d posts, log %q", p, out)
	}
}

// T-623: a cancel that fails past the age limit gives one line that names
// the run, and makes no more calls.
func TestACancelIsGivenUpPastTheAgeLimit(t *testing.T) {
	fastRetries(t, 150*time.Millisecond)
	api := newCancelAPI(t, 502)
	s := cancelSidecar(t, api)
	out := stderrOf(t, func() {
		s.cancelRefused(refusedJob(), "tier-mismatch")
		for end := time.Now().Add(400 * time.Millisecond); time.Now().Before(end); time.Sleep(5 * time.Millisecond) {
			nextOnce(s)
		}
	})
	p, _ := api.count()
	if strings.Count(out, "given up") != 1 || !strings.Contains(out, "run 5 of o/r") || len(s.cancelRetry) != 0 || p < 2 {
		t.Fatalf("%d posts, list %d, log %q", p, len(s.cancelRetry), out)
	}
	time.Sleep(100 * time.Millisecond)
	nextOnce(s)
	if p2, _ := api.count(); p2 != p {
		t.Fatalf("calls after the give-up: %d then %d", p, p2)
	}
}

// T-623: the list has a size limit: the oldest cancel goes with a line.
func TestTheCancelListIsBounded(t *testing.T) {
	fastRetries(t, time.Hour)
	cancelListMax = 3
	api := newCancelAPI(t, 502)
	s := cancelSidecar(t, api)
	out := stderrOf(t, func() {
		for run := int64(1); run <= 5; run++ {
			s.cancelRefused(&job{owner: "o", repo: "r", run: run}, "tier-mismatch")
		}
	})
	if len(s.cancelRetry) != 3 || s.cancelRetry[0].run != 3 || strings.Count(out, "given up") != 2 {
		t.Fatalf("list %d, log %q", len(s.cancelRetry), out)
	}
}

// T-623: a slow cancel endpoint does not push `next` past its budget.
func TestARetryOfACancelStaysInTheBudgetOfNext(t *testing.T) {
	fastRetries(t, time.Minute)
	defer func(a, b time.Duration) { lookupTimeout, nextBudget = a, b }(lookupTimeout, nextBudget)
	lookupTimeout, nextBudget = 200*time.Millisecond, 400*time.Millisecond
	api := newCancelAPI(t, 502, 202)
	s := cancelSidecar(t, api)
	s.cancelRefused(refusedJob(), "tier-mismatch")
	api.mu.Lock()
	api.delay = 5 * time.Second
	api.mu.Unlock()
	time.Sleep(30 * time.Millisecond)
	start := time.Now()
	nextOnce(s)
	took := time.Since(start)
	if p, _ := api.count(); p < 2 {
		t.Fatalf("the retry made no call (%d posts)", p)
	}
	if took > nextBudget+200*time.Millisecond {
		t.Fatalf("next took %v, the budget is %v", took, nextBudget)
	}
	if s.cancelled[5] || len(s.cancelRetry) != 1 {
		t.Fatalf("a call that the time ended is no answer: cancelled %v, list %d", s.cancelled[5], len(s.cancelRetry))
	}
}
