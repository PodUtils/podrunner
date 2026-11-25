package main

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"reflect"
	"strings"
	"sync"
	"testing"
	"time"
)

func TestRerunOnlyTheFailedJobOfTheJITRunner(t *testing.T) {
	var mu sync.Mutex
	var posts []string
	state := "in_progress"
	attempt := 1
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		mu.Lock()
		defer mu.Unlock()
		switch {
		case r.Method == http.MethodPost:
			posts = append(posts, r.URL.Path)
			w.WriteHeader(http.StatusCreated)
		case strings.HasSuffix(r.URL.Path, "/runs/7"):
			_ = json.NewEncoder(w).Encode(map[string]any{"id": 7, "run_attempt": attempt, "status": state})
		case strings.HasSuffix(r.URL.Path, "/runs/7/attempts/1/jobs"):
			jobs := []map[string]any{
				{"id": 81, "run_id": 7, "run_attempt": 1, "runner_id": 4, "status": "completed", "conclusion": "failure", "name": "check"},
				{"id": 82, "run_id": 7, "run_attempt": 1, "runner_id": 5, "status": "completed", "conclusion": "failure", "name": "check"},
			}
			if r.URL.Query().Get("page") == "1" {
				firstPage := make([]map[string]any, jobsPerPage)
				for i := range firstPage {
					firstPage[i] = jobs[0]
				}
				_ = json.NewEncoder(w).Encode(map[string]any{"total_count": jobsPerPage + 1, "jobs": firstPage})
			} else {
				_ = json.NewEncoder(w).Encode(map[string]any{"total_count": jobsPerPage + 1, "jobs": jobs[1:]})
			}
		default:
			w.WriteHeader(http.StatusNotFound)
		}
	}))
	defer server.Close()
	s := &sidecar{
		o:     options{configURL: "https://github.example/acme/project", operatorControlled: true},
		trust: &trust{api: server.URL, token: "fake", expires: time.Now().Add(time.Hour)},
		jobs:  map[string]*job{"j-1": {runner: 5, state: claimed}, "j-2": {runner: 5, state: claimed}},
	}
	line := func(job, repo string) []byte {
		return []byte(`{"schema":1,"op":"report","job_id":"` + job + `","report":{"assigned":{"verdict":"end","code":"cache-mismatch","repository":"` + repo + `","run_id":7}}}`)
	}
	now := time.Now()
	s.handle(line("j-1", "foreign/project"))
	s.handle(line("j-1", "acme/wrong-project"))
	if len(s.reruns) != 0 {
		t.Fatalf("queued jobs outside this source: %v", s.reruns)
	}
	s.handle(line("j-1", "acme/project"))
	s.handle(line("j-1", "acme/project"))
	if len(s.reruns) != 1 {
		t.Fatalf("wrong pending count: %v", s.reruns)
	}
	s.rerunMismatches(context.Background(), now.Add(time.Second))
	if len(s.reruns) != 1 {
		t.Fatal("the live workflow was dropped")
	}
	mu.Lock()
	state = "completed"
	mu.Unlock()
	s.rerunMismatches(context.Background(), now.Add(rerunInterval+2*time.Second))
	s.handle(line("j-1", "acme/project"))
	s.rerunMismatches(context.Background(), now.Add(2*rerunInterval+4*time.Second))
	mu.Lock()
	attempt = 2
	mu.Unlock()
	s.handle(line("j-2", "acme/project"))
	s.rerunMismatches(context.Background(), now.Add(3*rerunInterval))
	mu.Lock()
	defer mu.Unlock()
	if want := []string{"/repos/acme/project/actions/jobs/82/rerun"}; !reflect.DeepEqual(posts, want) {
		t.Fatalf("rerun calls: %v, want %v", posts, want)
	}
}
