package main

import (
	"encoding/json"
	"fmt"
	"os"
)

// Job is one job of the script.
type Job struct {
	// The runner request id of the scale set.
	RequestID     int64    `json:"requestId"`
	JobID         string   `json:"jobId"`
	Owner         string   `json:"owner"`
	Repo          string   `json:"repo"`
	WorkflowRunID int64    `json:"workflowRunId"`
	WorkflowRef   string   `json:"workflowRef"`
	Event         string   `json:"event"`
	Labels        []string `json:"labels"`
	Actor         string   `json:"actor"`
	HeadIsFork    bool     `json:"headIsFork"`
	Visibility    string   `json:"visibility"`
	ActorIsMember bool     `json:"actorIsMember"`
	// Cancel: the job ends with result "canceled" after it is assigned and
	// before it starts.
	Cancel bool `json:"cancel"`
	// Requeue: how many times an assigned job that no runner takes is
	// offered again (at most three).
	Requeue int `json:"requeue"`
}

// Script is the whole script.
type Script struct {
	ScaleSetName string `json:"scaleSetName"`
	Jobs         []Job  `json:"jobs"`
}

func readScript(path string) (*Script, error) {
	if path == "" {
		return &Script{ScaleSetName: "podrunner"}, nil
	}
	b, err := os.ReadFile(path)
	if err != nil {
		return nil, fmt.Errorf("the script: %w", err)
	}
	var s Script
	if err := json.Unmarshal(b, &s); err != nil {
		return nil, fmt.Errorf("the script: %w", err)
	}
	if s.ScaleSetName == "" {
		s.ScaleSetName = "podrunner"
	}
	for i := range s.Jobs {
		if s.Jobs[i].Requeue > 3 {
			s.Jobs[i].Requeue = 3
		}
	}
	return &s, nil
}
