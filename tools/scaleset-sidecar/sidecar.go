package main

import (
	"context"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"net"
	"os"
	"regexp"
	"sort"
	"strings"
	"sync"
	"time"
	"unicode/utf8"

	"github.com/actions/scaleset"
)

var jobIDPattern = regexp.MustCompile(`^[A-Za-z0-9._-]{1,128}$`)

type jobState int

const (
	offered jobState = iota
	acked
	claimed
)

type job struct {
	request int64
	// A job that GitHub assigned to the scale set: it is acquired already,
	// and it has no runner request (T-166).
	assigned bool
	state    jobState
	// The runner of the JIT configuration: a release removes it.
	runner int64
	// The run of the job, to cancel it after a refusal (T-169).
	owner, repo string
	run         int64
}

type sidecar struct {
	o       options
	ctx     context.Context
	client  *scaleset.Client
	session *scaleset.MessageSessionClient
	// closer ends the message session (the session; a stub in a test).
	closer interface{ Close(context.Context) error }
	poll   *poller
	trust  *trust
	jobs   map[string]*job
	// The offers that wait for capacity, in order.
	queue []queued
	// The job id of the wire for each jobId of GitHub (a GUID): the
	// messages of one job name it by its runner request, or only by its
	// jobId (T-166).
	byJobID map[string]string
	// The runs that the sidecar cancelled (T-169).
	cancelled map[int64]bool
	// The next call of each run with no final answer (queue.go).
	waits map[int64]*retryWait
	// The counts of the status line (report): the offers whose lookup gave
	// a final answer with no facts, and those past the limits.
	TrustLookupFailed, GivenUp int
	lastReport                 time.Time
	lastCounts                 [3]int
	// The statistics of the last message of GitHub.
	stats *scaleset.RunnerScaleSetStatistic
	// The names of the queued jobs, for the end of the sidecar.
	heldMu sync.Mutex
	held   []string
}

func newSidecar(ctx context.Context, o options, signer *signer) (*sidecar, error) {
	t := newTrust(o, signer)
	// The scope first: a refused scope gets no scale set on GitHub.
	if err := t.scope(o.configURL, o.personalAccount); err != nil {
		return nil, err
	}
	var client *scaleset.Client
	var err error
	if o.pat {
		// T-174: a fine-grained token of the organization, from descriptor 3.
		var tok string
		if tok, err = signer.pat(ctx); err != nil {
			return nil, err
		}
		client, err = scaleset.NewClientWithPersonalAccessToken(scaleset.NewClientWithPersonalAccessTokenConfig{GitHubConfigURL: o.configURL, PersonalAccessToken: tok})
	} else {
		client, err = scaleset.NewClientWithJWTProvider(
			scaleset.ClientWithJWTProviderConfig{GitHubConfigURL: o.configURL, InstallationID: o.installationID},
			scaleset.JWTProviderFunc(signer.token),
		)
	}
	if err != nil {
		return nil, fmt.Errorf("the client: %w", err)
	}
	set, err := client.GetRunnerScaleSet(ctx, 1, o.scaleSet)
	if err != nil {
		return nil, fmt.Errorf("the scale set %s: %s", o.scaleSet, statusOf(err))
	}
	if set == nil {
		if set, err = client.CreateRunnerScaleSet(ctx, &scaleset.RunnerScaleSet{Name: o.scaleSet, RunnerGroupID: 1}); err != nil {
			return nil, fmt.Errorf("create the scale set %s: %s", o.scaleSet, statusOf(err))
		}
	}
	session, err := client.MessageSessionClient(ctx, set.ID, "podrunner")
	if err != nil {
		return nil, fmt.Errorf("the message session: %s", statusOf(err))
	}
	if l := sessionStartLine(session.Session().Statistics); l != "" {
		logf("%s", l)
	}
	p := newPoller(session)
	go p.run(ctx)
	return &sidecar{o: o, ctx: ctx, client: client, session: session, closer: session, poll: p, trust: t, jobs: map[string]*job{}, byJobID: map[string]string{}}, nil
}

var statusPattern = regexp.MustCompile(`status="(\d{3})`)

// statusOf names the HTTP status of an error of the client, with no URL
// and no body: a body can hold a token.
func statusOf(err error) string {
	var rl *scaleset.RateLimitError
	if errors.As(err, &rl) {
		return "HTTP 429"
	}
	if m := statusPattern.FindStringSubmatch(err.Error()); m != nil {
		return "HTTP " + m[1]
	}
	return classOf(err)
}

// classOf names an error with no HTTP status by its class, with no URL and
// no body (T-168): each class names a cause that the operator can act on.
func classOf(err error) string {
	text := err.Error()
	var dns *net.DNSError
	var op *net.OpError
	switch {
	case strings.Contains(text, "sign refused: "):
		// The word of sign: rate, shape or key.
		w := text[strings.Index(text, "sign refused: ")+len("sign refused: "):]
		if i := strings.IndexAny(w, " :;,\n"); i >= 0 {
			w = w[:i]
		}
		return "sign refused: " + w
	case strings.Contains(text, "the sign channel"):
		return "the sign channel"
	case errors.Is(err, context.DeadlineExceeded) || strings.Contains(text, "Client.Timeout") || strings.Contains(text, "i/o timeout"):
		return "timeout"
	case errors.As(err, &dns):
		return "dns"
	case strings.Contains(text, "x509:") || strings.Contains(text, "tls:"):
		return "tls"
	case errors.As(err, &op) || strings.Contains(text, "connection refused") || strings.Contains(text, "connection reset"):
		return "connect"
	}
	return "no answer"
}

// codeOf maps an error of GitHub to a code of the source wire (call C15).
func codeOf(err error) string {
	switch statusOf(err) {
	case "HTTP 401", "HTTP 403":
		return "source-revoked"
	case "HTTP 404":
		return "unknown-job"
	case "HTTP 422":
		return "bad-request"
	default:
		return "unavailable"
	}
}

func fields(req map[string]any, allowed ...string) error {
	ok := map[string]bool{"schema": true, "op": true}
	for _, a := range allowed {
		ok[a] = true
	}
	for k := range req {
		if !ok[k] {
			return fmt.Errorf("an unknown field")
		}
	}
	return nil
}

func (s *sidecar) handle(line []byte) map[string]any {
	var req map[string]any
	if err := json.Unmarshal(line, &req); err != nil {
		return errorReply("bad-request", "not JSON")
	}
	if v, ok := req["schema"].(float64); !ok || v != 1 {
		return errorReply("bad-request", `no "schema": 1`)
	}
	op, _ := req["op"].(string)
	jobID, _ := req["job_id"].(string)
	if _, has := req["job_id"]; has && (!jobIDPattern.MatchString(jobID) || jobID == "." || jobID == "..") {
		return errorReply("bad-request", "a bad job_id")
	}
	runner, _ := req["runner_name"].(string)
	var err error
	switch op {
	case "hello":
		err = fields(req, "client", "version")
	case "next":
		err = fields(req, "capacity", "wait_s")
	case "ack", "release":
		err = fields(req, "job_id")
	case "claim", "mint":
		err = fields(req, "job_id", "runner_name")
	case "report":
		err = fields(req, "job_id", "report")
	case "audit":
		err = fields(req, "line")
	case "withdraw":
		err = fields(req, "job_id", "reason")
	default:
		return errorReply("bad-request", "an unknown op")
	}
	if err != nil {
		return errorReply("bad-request", err.Error())
	}
	switch op {
	case "hello":
		return map[string]any{"reply": "hello", "source": s.o.source, "operator_controlled": s.o.operatorControlled}
	case "next":
		capacity, _ := req["capacity"].(float64)
		wait, _ := req["wait_s"].(float64)
		return s.next(int(capacity), time.Duration(wait*float64(time.Second)))
	case "ack":
		return s.move(jobID, offered, acked)
	case "claim":
		return s.claim(jobID)
	case "mint":
		return s.mint(jobID, runner)
	case "withdraw":
		j := s.jobs[jobID]
		if j != nil && j.state == claimed {
			return errorReply("already-claimed", "release a claimed job")
		}
		if j != nil {
			reason, _ := req["reason"].(string)
			s.cancelRefused(j, reason)
		}
		s.forget(jobID)
		return map[string]any{"reply": "done"}
	case "release":
		j := s.jobs[jobID]
		if j == nil {
			return errorReply("unknown-job", "no offer of this job")
		}
		// A runner that never ran must not take another job (a revoked
		// host, GitHub #31). A runner that ran is gone: GitHub answers 404.
		if j.runner != 0 {
			_ = s.client.RemoveRunner(s.ctx, j.runner)
		}
		s.forget(jobID)
		return map[string]any{"reply": "done"}
	default: // report, audit: the scale set has no place for them.
		return map[string]any{"reply": "done"}
	}
}

func (s *sidecar) move(id string, from, to jobState) map[string]any {
	j := s.jobs[id]
	if j == nil {
		return errorReply("unknown-job", "no offer of this job")
	}
	if j.state == from {
		j.state = to
	}
	return map[string]any{"reply": "done"}
}

func (s *sidecar) claim(id string) map[string]any {
	j := s.jobs[id]
	switch {
	case j == nil:
		return errorReply("unknown-job", "no offer of this job")
	case j.state == claimed:
		return errorReply("already-claimed", "the job is claimed")
	case j.assigned:
		// GitHub acquired it for the scale set already.
		j.state = claimed
		return map[string]any{"reply": "done"}
	}
	got, err := s.session.AcquireJobs(s.ctx, []int64{j.request})
	if err != nil {
		return errorReply(codeOf(err), statusOf(err))
	}
	if len(got) != 1 || got[0] != j.request {
		delete(s.jobs, id)
		return errorReply("unknown-job", "another runner has the job")
	}
	j.state = claimed
	return map[string]any{"reply": "done"}
}

func (s *sidecar) mint(id, runner string) map[string]any {
	j := s.jobs[id]
	if j == nil {
		return errorReply("unknown-job", "no offer of this job")
	}
	if j.state != claimed {
		return errorReply("not-claimed", "claim the job first")
	}
	jit, err := s.client.GenerateJitRunnerConfig(s.ctx, &scaleset.RunnerScaleSetJitRunnerSetting{Name: runner, WorkFolder: "_work"}, 1)
	if err != nil {
		return errorReply(codeOf(err), statusOf(err))
	}
	if jit.Runner != nil {
		j.runner = int64(jit.Runner.ID)
	}
	files, err := decodeJIT(jit.EncodedJITConfig)
	if err != nil {
		return errorReply("credential-unavailable", err.Error())
	}
	return map[string]any{"reply": "credential", "credential": map[string]any{"files": files}}
}

// decodeJIT turns the JIT configuration (base64 of a JSON map of file
// names to contents) into files. A content that is base64 of UTF-8 text
// is decoded.
func decodeJIT(encoded string) ([]map[string]string, error) {
	raw, err := base64.StdEncoding.DecodeString(encoded)
	if err != nil {
		return nil, fmt.Errorf("the JIT configuration is not base64")
	}
	var m map[string]string
	if err := json.Unmarshal(raw, &m); err != nil {
		return nil, fmt.Errorf("the JIT configuration is not a map of files")
	}
	names := make([]string, 0, len(m))
	for k := range m {
		names = append(names, k)
	}
	sort.Strings(names)
	var files []map[string]string
	for _, name := range names {
		content := m[name]
		if b, err := base64.StdEncoding.DecodeString(content); err == nil && utf8.Valid(b) {
			content = string(b)
		}
		files = append(files, map[string]string{"name": name, "content": content})
	}
	return files, nil
}

// forget removes the job `id` and its jobId.
func (s *sidecar) forget(id string) {
	delete(s.jobs, id)
	for g, v := range s.byJobID {
		if v == id {
			delete(s.byJobID, g)
		}
	}
}

// closeSession ends the message session of the scale set, so that the next
// sidecar gets no 409 "already has an active session" (T-168).
func (s *sidecar) closeSession() {
	if l := s.heldLine(); l != "" {
		logf("%s", l)
	}
	if s.closer == nil {
		return
	}
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	if err := s.closer.Close(ctx); err != nil {
		fmt.Fprintln(os.Stderr, "podrunner-source-github-scaleset: close the message session:", statusOf(err))
	}
}
