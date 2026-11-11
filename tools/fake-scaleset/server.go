package main

import (
	"encoding/base64"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"os"
	"strconv"
	"strings"
	"sync"
	"time"
)

// The state of one job in the fake service.
type jobState struct {
	Job
	offered, acquired, assigned, started, completed bool
	runner                                          string
	requeues                                        int
	result                                          string
}

type server struct {
	mu      sync.Mutex
	base    string
	script  *Script
	jobs    []*jobState
	created bool
	session string
	// The message that was delivered and not deleted: it comes again.
	pending   map[string]any
	nextMsg   int
	runners   []string
	record    *os.File
	pollDelay time.Duration
}

func newServer(base string, s *Script, record string) (*server, error) {
	srv := &server{base: base, script: s, nextMsg: 1, pollDelay: 200 * time.Millisecond}
	for _, j := range s.Jobs {
		srv.jobs = append(srv.jobs, &jobState{Job: j})
	}
	if record != "" {
		f, err := os.OpenFile(record, os.O_CREATE|os.O_WRONLY|os.O_APPEND, 0o600)
		if err != nil {
			return nil, err
		}
		srv.record = f
	}
	return srv, nil
}

func reply(w http.ResponseWriter, status int, v any) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	if v != nil {
		_ = json.NewEncoder(w).Encode(v)
	}
}

// A token with an expiry that the client reads unverified.
func adminToken() string {
	enc := base64.RawURLEncoding.EncodeToString
	claims := fmt.Sprintf(`{"exp":%d,"iat":%d,"iss":"fake"}`, time.Now().Add(time.Hour).Unix(), time.Now().Unix())
	return enc([]byte(`{"alg":"none","typ":"JWT"}`)) + "." + enc([]byte(claims)) + "." + enc([]byte("fake"))
}

func (s *server) scaleSet() map[string]any {
	return map[string]any{
		"id": 1, "name": s.script.ScaleSetName, "runnerGroupId": 1, "runnerGroupName": "default",
		"labels":        []map[string]string{{"type": "System", "name": s.script.ScaleSetName}},
		"RunnerSetting": map[string]any{}, "createdOn": time.Now().UTC(),
	}
}

func (s *server) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.record != nil {
		line, _ := json.Marshal(map[string]any{
			"method": r.Method, "path": r.URL.Path, "query": r.URL.RawQuery,
			"capacity": r.Header.Get("X-ScaleSetMaxCapacity"),
		})
		_, _ = s.record.Write(append(line, '\n'))
	}
	p := r.URL.Path
	switch {
	case strings.HasSuffix(p, "/access_tokens") && strings.Contains(p, "/app/installations/"):
		reply(w, http.StatusCreated, map[string]any{"token": "ghs-fake", "expires_at": time.Now().Add(time.Hour).UTC()})
	case strings.HasSuffix(p, "/runners/registration-token"):
		reply(w, http.StatusCreated, map[string]any{"token": "fake-registration", "expires_at": time.Now().Add(time.Hour).UTC()})
	case strings.HasSuffix(p, "/actions/runner-registration"):
		reply(w, http.StatusCreated, map[string]any{"url": s.base + "/tenant/1/", "token": adminToken()})
	case strings.Contains(p, "/actions/runs/"):
		s.workflowRun(w, p)
	case strings.Contains(p, "/members/"):
		s.member(w, p)
	case strings.HasPrefix(p, "/tenant/1/_apis/runtime/runnergroups"):
		reply(w, http.StatusOK, map[string]any{"count": 1, "value": []map[string]any{{"id": 1, "name": "default", "isDefaultGroup": true}}})
	case strings.HasPrefix(p, "/tenant/1/_apis/runtime/runnerscalesets"):
		s.scaleSets(w, r, strings.TrimPrefix(p, "/tenant/1/_apis/runtime/runnerscalesets"))
	case strings.HasPrefix(p, "/tenant/1/_apis/distributedtask/pools/0/agents"):
		s.agents(w, r)
	case strings.HasPrefix(p, "/queue/1"):
		s.queue(w, r, strings.TrimPrefix(p, "/queue/1"))
	case strings.HasPrefix(p, "/fake/complete/"):
		s.complete(w, r, strings.TrimPrefix(p, "/fake/complete/"))
	default:
		reply(w, http.StatusNotFound, map[string]string{"message": "Not Found"})
	}
}

func (s *server) scaleSets(w http.ResponseWriter, r *http.Request, rest string) {
	switch {
	case rest == "" || rest == "/":
		if r.Method == http.MethodPost {
			s.created = true
			reply(w, http.StatusOK, s.scaleSet())
			return
		}
		if !s.created {
			reply(w, http.StatusOK, map[string]any{"count": 0, "value": []any{}})
			return
		}
		reply(w, http.StatusOK, map[string]any{"count": 1, "value": []any{s.scaleSet()}})
	case rest == "/1":
		if r.Method == http.MethodDelete {
			s.created = false
			w.WriteHeader(http.StatusNoContent)
			return
		}
		reply(w, http.StatusOK, s.scaleSet())
	case strings.HasPrefix(rest, "/1/sessions"):
		if r.Method == http.MethodDelete {
			s.session = ""
			w.WriteHeader(http.StatusNoContent)
			return
		}
		s.session = "00000000-0000-0000-0000-000000000001"
		reply(w, http.StatusOK, map[string]any{
			"sessionId": s.session, "ownerName": "podrunner", "runnerScaleSet": s.scaleSet(),
			"messageQueueUrl": s.base + "/queue/1", "messageQueueAccessToken": "fake-queue-token",
			"statistics": s.stats(),
		})
	case rest == "/1/acquirejobs":
		var ids []int64
		_ = json.NewDecoder(io.LimitReader(r.Body, 1<<20)).Decode(&ids)
		got := []int64{}
		for _, id := range ids {
			for _, j := range s.jobs {
				if j.RequestID == id && j.offered && !j.acquired && !j.completed {
					j.acquired = true
					got = append(got, id)
				}
			}
		}
		reply(w, http.StatusOK, map[string]any{"count": len(got), "value": got})
	case rest == "/1/generatejitconfig":
		var setting struct {
			Name string `json:"name"`
		}
		_ = json.NewDecoder(io.LimitReader(r.Body, 1<<20)).Decode(&setting)
		s.runners = append(s.runners, setting.Name)
		files := map[string]string{".runner": `{"agentName":"` + setting.Name + `"}`, ".credentials": `{"scheme":"OAuth"}`, ".credentials_rsaparams": `{}`}
		raw, _ := json.Marshal(files)
		reply(w, http.StatusOK, map[string]any{
			"runner":           map[string]any{"id": len(s.runners), "name": setting.Name, "runnerScaleSetId": 1},
			"encodedJITConfig": base64.StdEncoding.EncodeToString(raw),
		})
	default:
		reply(w, http.StatusNotFound, map[string]string{"message": "Not Found"})
	}
}

func (s *server) agents(w http.ResponseWriter, r *http.Request) {
	if r.Method == http.MethodDelete {
		w.WriteHeader(http.StatusNoContent)
		return
	}
	reply(w, http.StatusOK, map[string]any{"count": 0, "value": []any{}})
}

func (s *server) stats() map[string]int {
	var avail, assigned, running int
	for _, j := range s.jobs {
		switch {
		case j.completed:
		case j.started:
			running++
		case j.assigned:
			assigned++
		case j.offered:
			avail++
		}
	}
	return map[string]int{"totalAvailableJobs": avail, "totalAssignedJobs": assigned, "totalRunningJobs": running}
}

func (s *server) msgBase(j *jobState, kind string) map[string]any {
	return map[string]any{
		"messageType": kind, "runnerRequestId": j.RequestID, "repositoryName": j.Repo, "ownerName": j.Owner,
		"jobId": j.JobID, "jobWorkflowRef": j.WorkflowRef, "jobDisplayName": j.JobID, "workflowRunId": j.WorkflowRunID,
		"eventName": j.Event, "requestLabels": j.Labels, "queueTime": time.Now().UTC(),
	}
}

// The next message: the one that was not deleted, or the new events.
func (s *server) nextMessage() map[string]any {
	if s.pending != nil {
		return s.pending
	}
	var body []map[string]any
	for _, j := range s.jobs {
		switch {
		case j.completed:
		case !j.offered:
			j.offered = true
			m := s.msgBase(j, "JobAvailable")
			m["acquireJobUrl"] = s.base + "/tenant/1/_apis/runtime/runnerscalesets/1/acquirejobs"
			body = append(body, m)
		case j.acquired && !j.assigned:
			j.assigned = true
			body = append(body, s.msgBase(j, "JobAssigned"))
			if j.Cancel {
				j.completed, j.result = true, "canceled"
				m := s.msgBase(j, "JobCompleted")
				m["result"] = "canceled"
				body = append(body, m)
			}
		case j.assigned && !j.started && j.runner == "" && len(s.runners) > s.startedCount():
			j.runner = s.runners[s.startedCount()]
			j.started = true
			m := s.msgBase(j, "JobStarted")
			m["runnerName"], m["runnerId"] = j.runner, s.startedCount()
			body = append(body, m)
		case j.assigned && !j.started && j.requeues < j.Requeue:
			// No runner took the job: it is offered again.
			j.requeues++
			j.acquired, j.assigned = false, false
			m := s.msgBase(j, "JobAvailable")
			m["acquireJobUrl"] = s.base + "/tenant/1/_apis/runtime/runnerscalesets/1/acquirejobs"
			body = append(body, m)
		case j.started && j.result != "":
			j.completed = true
			m := s.msgBase(j, "JobCompleted")
			m["result"], m["runnerName"] = j.result, j.runner
			body = append(body, m)
		}
	}
	if len(body) == 0 {
		return nil
	}
	raw, _ := json.Marshal(body)
	msg := map[string]any{"messageId": s.nextMsg, "messageType": "RunnerScaleSetJobMessages", "body": string(raw), "statistics": s.stats()}
	s.nextMsg++
	s.pending = msg
	return msg
}

func (s *server) startedCount() int {
	n := 0
	for _, j := range s.jobs {
		if j.started {
			n++
		}
	}
	return n
}

func (s *server) queue(w http.ResponseWriter, r *http.Request, rest string) {
	if r.Method == http.MethodDelete {
		id, _ := strconv.Atoi(strings.TrimPrefix(rest, "/"))
		if s.pending != nil && s.pending["messageId"] == id {
			s.pending = nil
		}
		w.WriteHeader(http.StatusNoContent)
		return
	}
	if r.Header.Get("Authorization") != "Bearer fake-queue-token" {
		w.WriteHeader(http.StatusUnauthorized)
		return
	}
	if m := s.nextMessage(); m != nil {
		reply(w, http.StatusOK, m)
		return
	}
	// A short long poll: the tests wait no 50 s.
	s.mu.Unlock()
	time.Sleep(s.pollDelay)
	s.mu.Lock()
	w.WriteHeader(http.StatusAccepted)
}

// POST /fake/complete/ID?result=succeeded: the runner of a job ends.
func (s *server) complete(w http.ResponseWriter, r *http.Request, id string) {
	n, _ := strconv.ParseInt(id, 10, 64)
	for _, j := range s.jobs {
		if j.RequestID == n {
			j.result = r.URL.Query().Get("result")
			if j.result == "" {
				j.result = "succeeded"
			}
		}
	}
	w.WriteHeader(http.StatusNoContent)
}

func (s *server) job(owner, repo string, run int64) *jobState {
	for _, j := range s.jobs {
		if j.Owner == owner && j.Repo == repo && j.WorkflowRunID == run {
			return j
		}
	}
	return nil
}

// GET .../repos/OWNER/REPO/actions/runs/ID: the trust inputs of a run.
func (s *server) workflowRun(w http.ResponseWriter, p string) {
	parts := strings.Split(p[strings.Index(p, "/repos/")+len("/repos/"):], "/")
	if len(parts) < 5 {
		reply(w, http.StatusNotFound, nil)
		return
	}
	run, _ := strconv.ParseInt(parts[4], 10, 64)
	j := s.job(parts[0], parts[1], run)
	if j == nil {
		reply(w, http.StatusNotFound, map[string]string{"message": "Not Found"})
		return
	}
	reply(w, http.StatusOK, map[string]any{
		"id": run, "event": j.Event, "actor": map[string]any{"login": j.Actor},
		"head_repository": map[string]any{"fork": j.HeadIsFork},
		"repository":      map[string]any{"name": j.Repo, "owner": map[string]any{"login": j.Owner}, "visibility": j.Visibility, "private": j.Visibility != "public"},
	})
}

// GET .../orgs/ORG/members/USER: 204 for a member, 404 else.
func (s *server) member(w http.ResponseWriter, p string) {
	user := p[strings.LastIndex(p, "/")+1:]
	for _, j := range s.jobs {
		if j.Actor == user && j.ActorIsMember {
			w.WriteHeader(http.StatusNoContent)
			return
		}
	}
	w.WriteHeader(http.StatusNotFound)
}
