package main

import (
	"context"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"regexp"
	"sort"
	"strconv"
	"strings"
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
	state   jobState
	// The runner of the JIT configuration: a release removes it.
	runner int64
}

type sidecar struct {
	o       options
	ctx     context.Context
	client  *scaleset.Client
	session *scaleset.MessageSessionClient
	trust   *trust
	jobs    map[string]*job
	// The offers that wait for capacity, in order.
	queue  []map[string]any
	lastID int
	// The counts of status.
	TrustLookupFailed int
}

func newSidecar(ctx context.Context, o options, signer *signer) (*sidecar, error) {
	client, err := scaleset.NewClientWithJWTProvider(
		scaleset.ClientWithJWTProviderConfig{GitHubConfigURL: o.configURL, InstallationID: o.installationID},
		scaleset.JWTProviderFunc(func(ctx context.Context) (string, error) { return signer.token(o.installationID) }),
	)
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
	t := &trust{api: o.apiURL, installation: o.installationID, signer: signer, cache: map[int64]trustInputs{}, budget: o.trustBudget}
	owner := strings.TrimSuffix(o.configURL, "/")
	owner = owner[strings.LastIndex(owner, "/")+1:]
	if err := t.organization(owner); err != nil {
		return nil, err
	}
	return &sidecar{o: o, ctx: ctx, client: client, session: session, trust: t, jobs: map[string]*job{}}, nil
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
		return s.next(int(capacity))
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
		delete(s.jobs, jobID)
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
		delete(s.jobs, jobID)
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

func (s *sidecar) next(capacity int) map[string]any {
	events := []any{}
	msg, err := s.session.GetMessage(s.ctx, s.lastID, capacity)
	if err != nil {
		return errorReply(codeOf(err), statusOf(err))
	}
	if msg != nil {
		for _, a := range msg.JobAvailableMessages {
			id := strconv.FormatInt(a.RunnerRequestID, 10)
			if s.jobs[id] != nil {
				continue // a redelivery
			}
			s.queue = append(s.queue, s.offer(id, a))
		}
		for _, st := range msg.JobStartedMessages {
			events = append(events, map[string]any{"event": "started", "job_id": strconv.FormatInt(st.RunnerRequestID, 10), "runner_name": st.RunnerName})
		}
		for _, c := range msg.JobCompletedMessages {
			id := strconv.FormatInt(c.RunnerRequestID, 10)
			if c.Result == "canceled" || c.Result == "cancelled" {
				events = append(events, map[string]any{"event": "cancel", "job_id": id})
			} else {
				events = append(events, map[string]any{"event": "completed", "job_id": id, "result": c.Result})
			}
		}
		if err := s.session.DeleteMessage(s.ctx, msg.MessageID); err == nil {
			s.lastID = msg.MessageID
		}
	}
	for capacity > 0 && len(s.queue) > 0 {
		o := s.queue[0]
		s.queue = s.queue[1:]
		id := o["job_id"].(string)
		req, _ := strconv.ParseInt(id, 10, 64)
		s.jobs[id] = &job{request: req, state: offered}
		events = append(events, map[string]any{"event": "offer", "offer": o})
		capacity--
	}
	return map[string]any{"reply": "events", "events": events}
}

// offer builds job-offer.v1 from a JobAvailable message and the trust
// lookup. It has no tier and no payload.
func (s *sidecar) offer(id string, a *scaleset.JobAvailable) map[string]any {
	t, ok := s.trust.lookup(a.OwnerName, a.RepositoryName, a.WorkflowRunID, a.EventName)
	if !ok {
		s.TrustLookupFailed++
	}
	labels := a.RequestLabels
	if labels == nil {
		labels = []string{}
	}
	return map[string]any{
		"schema": 1, "job_id": id, "source": s.o.source, "labels": labels, "arch": s.o.arch,
		"wait_deadline_s": s.o.waitDeadlineS,
		"trust_inputs": map[string]any{
			"event": t.event, "head_is_fork": t.headIsFork, "actor_class": t.actorClass,
			"repo_visibility": t.visibility, "owner": a.OwnerName, "repo": a.RepositoryName,
			"run_id": a.WorkflowRunID, "ref": "", "workflow_ref": a.JobWorkflowRef,
		},
	}
}
