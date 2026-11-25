package main

import (
	"bufio"
	"context"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func TestRerunWithFakeScaleSet(t *testing.T) {
	binary := os.Getenv("PODRUNNER_FAKE_SCALESET")
	if binary == "" {
		t.Skip("set PODRUNNER_FAKE_SCALESET to the compiled fake")
	}
	t.Setenv("GITHUB_ACTIONS_FORCE_GHES", "true")
	ctx, stop := context.WithTimeout(context.Background(), 20*time.Second)
	defer stop()
	dir := t.TempDir()
	fixture := filepath.Join(dir, "jobs.json")
	record := filepath.Join(dir, "requests.jsonl")
	if err := os.WriteFile(fixture, []byte(`{"jobs":[{"requestId":37,"jobId":"g-1","owner":"example-org","repo":"app","workflowRunId":7,"event":"push","actor":"alice","actorIsMember":true,"visibility":"private","assignedOnly":true}]}`), 0o600); err != nil {
		t.Fatal(err)
	}
	cmd := exec.CommandContext(ctx, binary, "-script", fixture, "-record", record)
	stdout, err := cmd.StdoutPipe()
	if err != nil {
		t.Fatal(err)
	}
	if err := cmd.Start(); err != nil {
		t.Fatal(err)
	}
	defer func() { _ = cmd.Process.Kill(); _ = cmd.Wait() }()
	lines := bufio.NewScanner(stdout)
	if !lines.Scan() {
		t.Fatalf("fake scale set did not publish its address: %v", lines.Err())
	}
	base := lines.Text()
	s, err := newSidecar(ctx, options{configURL: base + "/example-org", apiURL: base + "/api/v3",
		scaleSet: "podrunner", source: "s", arch: "x86_64", installationID: 7,
		operatorControlled: true}, &signer{cached: "a.b.c", until: farFuture})
	if err != nil {
		t.Fatal(err)
	}
	defer s.closeSession()
	const key = "a-g-1"
	for ctx.Err() == nil && s.jobs[key] == nil {
		s.next(1, 300*time.Millisecond)
	}
	if s.jobs[key] == nil {
		t.Fatal("the fake never offered its job")
	}
	for _, exchange := range []struct{ line, reply string }{
		{`{"schema":1,"op":"ack","job_id":"a-g-1"}`, "done"},
		{`{"schema":1,"op":"claim","job_id":"a-g-1","runner_name":"runner-1"}`, "done"},
		{`{"schema":1,"op":"mint","job_id":"a-g-1","runner_name":"runner-1"}`, "credential"},
	} {
		if reply := s.handle([]byte(exchange.line)); reply["reply"] != exchange.reply {
			t.Fatalf("%s: %v", exchange.line, reply)
		}
	}
	if s.jobs[key].runner <= 0 {
		t.Fatal("the fake did not return a JIT runner ID")
	}
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, base+"/fake/complete/37?result=failed", nil)
	if err != nil {
		t.Fatal(err)
	}
	res, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	res.Body.Close()
	if res.StatusCode != http.StatusNoContent {
		t.Fatalf("marking the fake job failed returned %d", res.StatusCode)
	}
	line := []byte(`{"schema":1,"op":"report","job_id":"a-g-1","report":{"assigned":{"verdict":"end","code":"cache-mismatch","repository":"example-org/app","run_id":7}}}`)
	if reply := s.handle(line); reply["reply"] != "done" || len(s.reruns) != 1 {
		t.Fatalf("report: %v, reruns: %v", reply, s.reruns)
	}
	for ctx.Err() == nil && len(s.reruns) > 0 {
		s.next(0, 300*time.Millisecond)
		s.rerunMismatches(ctx, time.Now().Add(rerunInterval))
	}
	s.handle(line)
	s.rerunMismatches(ctx, time.Now().Add(2*rerunInterval))
	calls, err := os.ReadFile(record)
	if err != nil {
		t.Fatal(err)
	}
	if got := strings.Count(string(calls), `"path":"/api/v3/repos/example-org/app/actions/jobs/37/rerun"`); got != 1 {
		t.Fatalf("wanted one rerun, got %d", got)
	}
	obj, err := s.trust.runNow(ctx, "example-org", "app", 7)
	if err != nil || numericRestID(obj["run_attempt"]) != 2 {
		t.Fatalf("the fake did not start the second attempt: %v, %v", obj, err)
	}
}
