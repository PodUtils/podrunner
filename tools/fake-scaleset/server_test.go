package main

import (
	"context"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"github.com/actions/scaleset"
)

func start(t *testing.T, s *Script) (*httptest.Server, *server) {
	t.Helper()
	ts := httptest.NewUnstartedServer(nil)
	ts.Start()
	srv, err := newServer(ts.URL, s, "")
	if err != nil {
		t.Fatal(err)
	}
	srv.pollDelay = 10 * time.Millisecond
	ts.Config.Handler = srv
	t.Cleanup(ts.Close)
	return ts, srv
}

func client(t *testing.T, base string) *scaleset.Client {
	t.Helper()
	c, err := scaleset.NewClientWithJWTProvider(
		scaleset.ClientWithJWTProviderConfig{GitHubConfigURL: base + "/example-org", InstallationID: 7},
		scaleset.JWTProviderFunc(func(context.Context) (string, error) { return "a.b.c", nil }),
	)
	if err != nil {
		t.Fatal(err)
	}
	return c
}

func script() *Script {
	return &Script{ScaleSetName: "podrunner", Jobs: []Job{
		{RequestID: 11, JobID: "job-11", Owner: "example-org", Repo: "app", WorkflowRunID: 101, Event: "push", Actor: "alice", ActorIsMember: true, Visibility: "private"},
		{RequestID: 12, JobID: "job-12", Owner: "example-org", Repo: "app", WorkflowRunID: 102, Event: "pull_request", Actor: "mallory", HeadIsFork: true, Visibility: "public"},
		{RequestID: 13, JobID: "job-13", Owner: "example-org", Repo: "app", WorkflowRunID: 103, Event: "push", Actor: "alice", ActorIsMember: true, Cancel: true},
	}}
}

// The real client of actions/scaleset: a scale set, a session, the
// messages of a job from JobAvailable to JobCompleted, a redelivery of a
// message that was not deleted, a fork pull request and a cancel.
func TestTheClientOfActionsScalesetSpeaksToTheFake(t *testing.T) {
	ts, srv := start(t, script())
	ctx := context.Background()
	c := client(t, ts.URL)
	set, err := c.CreateRunnerScaleSet(ctx, &scaleset.RunnerScaleSet{Name: "podrunner", RunnerGroupID: 1})
	if err != nil || set.ID != 1 {
		t.Fatalf("create: %v %+v", err, set)
	}
	sess, err := c.MessageSessionClient(ctx, set.ID, "podrunner")
	if err != nil {
		t.Fatalf("session: %v", err)
	}
	msg, err := sess.GetMessage(ctx, 0, 2)
	if err != nil || msg == nil || len(msg.JobAvailableMessages) != 3 {
		t.Fatalf("the first message: %v %+v", err, msg)
	}
	if msg.JobAvailableMessages[1].EventName != "pull_request" {
		t.Fatalf("the fork pull request: %+v", msg.JobAvailableMessages[1])
	}
	// Not deleted: the same message comes again.
	again, err := sess.GetMessage(ctx, 0, 2)
	if err != nil || again == nil || again.MessageID != msg.MessageID {
		t.Fatalf("a redelivery: %v %+v", err, again)
	}
	if err := sess.DeleteMessage(ctx, msg.MessageID); err != nil {
		t.Fatal(err)
	}
	got, err := sess.AcquireJobs(ctx, []int64{11, 13})
	if err != nil || len(got) != 2 {
		t.Fatalf("acquire: %v %v", err, got)
	}
	jit, err := c.GenerateJitRunnerConfig(ctx, &scaleset.RunnerScaleSetJitRunnerSetting{Name: "podrunner-host-a-u-0"}, set.ID)
	if err != nil || jit.Runner.Name != "podrunner-host-a-u-0" || jit.EncodedJITConfig == "" {
		t.Fatalf("jit: %v %+v", err, jit)
	}
	var started, cancelled bool
	for i := 0; i < 10 && !(started && cancelled); i++ {
		m, err := sess.GetMessage(ctx, msg.MessageID, 2)
		if err != nil {
			t.Fatal(err)
		}
		if m == nil {
			continue
		}
		for _, s := range m.JobStartedMessages {
			started = started || (s.RunnerName == "podrunner-host-a-u-0" && s.RunnerRequestID == 11)
		}
		for _, c := range m.JobCompletedMessages {
			cancelled = cancelled || (c.RunnerRequestID == 13 && c.Result == "canceled")
		}
		_ = sess.DeleteMessage(ctx, m.MessageID)
		msg = m
	}
	if !started || !cancelled {
		t.Fatalf("started %v, cancelled %v", started, cancelled)
	}
	resp, err := http.Post(ts.URL+"/fake/complete/11?result=succeeded", "text/plain", strings.NewReader(""))
	if err != nil {
		t.Fatal(err)
	}
	resp.Body.Close()
	m, err := sess.GetMessage(ctx, msg.MessageID, 2)
	if err != nil || m == nil || len(m.JobCompletedMessages) != 1 || m.JobCompletedMessages[0].Result != "succeeded" {
		t.Fatalf("completed: %v %+v", err, m)
	}
	_ = srv
}

func TestTheTrustInputsOfARun(t *testing.T) {
	ts, _ := start(t, script())
	for path, want := range map[string]int{
		"/api/v3/repos/example-org/app/actions/runs/102": 200,
		"/api/v3/repos/example-org/app/actions/runs/999": 404,
		"/api/v3/orgs/example-org/members/alice":         204,
		"/api/v3/orgs/example-org/members/mallory":       404,
	} {
		resp, err := http.Get(ts.URL + path)
		if err != nil {
			t.Fatal(err)
		}
		resp.Body.Close()
		if resp.StatusCode != want {
			t.Errorf("%s: %d, not %d", path, resp.StatusCode, want)
		}
	}
}
