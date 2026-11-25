package main

import (
	"net/http"
	"strconv"
	"strings"
)

func (s *server) runStatus(owner, repo string, run int64) string {
	for _, j := range s.jobs {
		if j.Owner == owner && j.Repo == repo && j.WorkflowRunID == run && !j.completed {
			return "in_progress"
		}
	}
	return "completed"
}

// The fake uses its request ID as the numeric REST workflow job ID.
func (s *server) workflowJobs(w http.ResponseWriter, path string) {
	_, tail, _ := strings.Cut(path, "/repos/")
	parts := strings.Split(tail, "/")
	if len(parts) != 8 {
		reply(w, http.StatusNotFound, nil)
		return
	}
	run, _ := strconv.ParseInt(parts[4], 10, 64)
	jobs := []map[string]any{}
	for _, j := range s.jobs {
		if j.Owner != parts[0] || j.Repo != parts[1] || j.WorkflowRunID != run {
			continue
		}
		status := "in_progress"
		if j.completed {
			status = "completed"
		}
		conclusion := map[string]string{"failed": "failure", "succeeded": "success", "canceled": "cancelled"}[j.result]
		jobs = append(jobs, map[string]any{"id": j.RequestID, "run_id": run, "run_attempt": 1,
			"status": status, "conclusion": conclusion, "runner_id": j.runnerID})
	}
	reply(w, http.StatusOK, map[string]any{"total_count": len(jobs), "jobs": jobs})
}

func (s *server) rerunWorkflowJob(w http.ResponseWriter, path string) {
	_, tail, _ := strings.Cut(path, "/repos/")
	parts := strings.Split(tail, "/")
	if len(parts) == 6 {
		id, _ := strconv.ParseInt(parts[4], 10, 64)
		for _, j := range s.jobs {
			if j.Owner != parts[0] || j.Repo != parts[1] || j.RequestID != id {
				continue
			}
			if j.RunAttempt > 1 || !j.completed || j.result != "failed" || s.runStatus(j.Owner, j.Repo, j.WorkflowRunID) != "completed" {
				reply(w, http.StatusConflict, nil)
				return
			}
			for _, same := range s.jobs {
				if same.Owner == j.Owner && same.Repo == j.Repo && same.WorkflowRunID == j.WorkflowRunID {
					same.RunAttempt = 2
				}
			}
			reply(w, http.StatusCreated, nil)
			return
		}
	}
	reply(w, http.StatusNotFound, nil)
}
