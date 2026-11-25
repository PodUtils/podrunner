package main

import (
	"context"
	"fmt"
	"net/http"
	"strings"
	"time"
)

const rerunQueueMax = 128
const rerunMaxAge = 60 * time.Minute
const rerunInterval = 15 * time.Second
const jobsPerPage = 30
const maxJobPages = 50

// A mismatch report identifies the run and repository. It does not give
// the REST job ID. GitHub's numeric ID of the minted JIT runner pins the
// job of that run even when a matrix repeats a display name.
type rerunMismatch struct {
	owner, repo string
	run, runner int64
	first, next time.Time
	page        int
}

func (s *sidecar) queueMismatchRerun(id string, report map[string]any, now time.Time) {
	j := s.jobs[id]
	if !s.o.operatorControlled || j == nil || j.state != claimed || j.runner <= 0 || j.rerunQueued {
		return
	}
	assigned, _ := report["assigned"].(map[string]any)
	if assigned["code"] != "cache-mismatch" || assigned["verdict"] != "end" {
		return
	}
	repository, _ := assigned["repository"].(string)
	owner, repo, separated := strings.Cut(repository, "/")
	parts, err := urlParts(s.o.configURL)
	run := numericRestID(assigned["run_id"])
	if err != nil || !separated || len(repo) > 100 || !nameOK(repo) || !asciiFold(owner, parts[0]) ||
		(len(parts) == 2 && !asciiFold(repo, parts[1])) || run == 0 {
		return
	}
	if len(s.reruns) >= rerunQueueMax {
		logf("cache-mismatch: the pending rerun queue is full for %s/%s run %d", owner, repo, run)
		return
	}
	j.rerunQueued = true
	s.reruns = append(s.reruns, rerunMismatch{owner: owner, repo: repo, run: run, runner: j.runner, first: now, next: now, page: 1})
}

func numericRestID(value any) int64 {
	const max = 1<<53 - 1
	if n, ok := value.(float64); ok && n >= 1 && n <= max && n == float64(int64(n)) {
		return int64(n)
	}
	return 0
}

// At most one candidate per poll. Return it to the queue while GitHub
// still runs the first attempt or could not read the run yet.
func (s *sidecar) rerunMismatches(ctx context.Context, now time.Time) {
	for i, r := range s.reruns {
		if ctx.Err() != nil || now.Before(r.next) {
			continue
		}
		s.reruns = append(s.reruns[:i], s.reruns[i+1:]...)
		if now.Sub(r.first) >= rerunMaxAge {
			r.log("gave up waiting for the failed job")
		} else if s.tryMismatchRerun(ctx, &r) {
			r.next = now.Add(rerunInterval)
			s.reruns = append(s.reruns, r)
		}
		return
	}
}

func (r rerunMismatch) log(detail string) {
	logf("cache-mismatch: %s/%s run %d, runner %d: %s", r.owner, r.repo, r.run, r.runner, detail)
}

// True when a later read can settle the candidate. The POST is made once:
// a transport error can mean GitHub received it and the reply went away.
func (s *sidecar) tryMismatchRerun(ctx context.Context, r *rerunMismatch) bool {
	obj, err := s.trust.runNow(ctx, r.owner, r.repo, r.run)
	if err != nil {
		return true
	}
	if numericRestID(obj["id"]) != r.run || numericRestID(obj["run_attempt"]) != 1 {
		r.log("the first attempt is unavailable")
		return false
	}
	if obj["status"] != "completed" {
		return true
	}
	root := fmt.Sprintf("/repos/%s/%s/actions", r.owner, r.repo)
	id, again := s.mismatchJobID(ctx, root, r)
	if again {
		return true
	}
	if id == 0 {
		r.log("no failed workflow job matched this runner")
		return false
	}
	if ctx.Err() != nil {
		return true
	}
	status, _ := s.trust.post(ctx, fmt.Sprintf("%s/jobs/%d/rerun", root, id))
	r.log(fmt.Sprintf("rerun of workflow job %d returned HTTP %d", id, status))
	return false
}

// Read one page per poll, so a wide matrix cannot spend the wait of an offer.
func (s *sidecar) mismatchJobID(ctx context.Context, root string, r *rerunMismatch) (int64, bool) {
	path := fmt.Sprintf("%s/runs/%d/attempts/1/jobs?per_page=%d&page=%d", root, r.run, jobsPerPage, r.page)
	s.trust.mu.Lock()
	status, obj, _, err := s.trust.get(ctx, path, "")
	s.trust.mu.Unlock()
	if err != nil {
		return 0, true
	}
	if status != http.StatusOK {
		return 0, false
	}
	jobs, _ := obj["jobs"].([]any)
	for _, row := range jobs {
		job, _ := row.(map[string]any)
		if numericRestID(job["runner_id"]) == r.runner && numericRestID(job["run_id"]) == r.run &&
			numericRestID(job["run_attempt"]) == 1 && job["status"] == "completed" && job["conclusion"] == "failure" {
			return numericRestID(job["id"]), false
		}
	}
	if len(jobs) == jobsPerPage && r.page < maxJobPages {
		r.page++
		return 0, true
	}
	return 0, false
}
