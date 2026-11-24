package main

import (
	"bytes"
	"encoding/base64"
	"encoding/json"
	"errors"
	"strings"
	"testing"
)

// Each status of GitHub maps to one code of the source wire (call C15),
// and the detail names the status only: no URL, no body.
func TestTheStatusesOfGitHubMapToCodes(t *testing.T) {
	cases := map[string]string{
		"401 Unauthorized":          "source-revoked",
		"403 Forbidden":             "source-revoked",
		"404 Not Found":             "unknown-job",
		"422 Unprocessable Entity":  "bad-request",
		"429 Too Many Requests":     "unavailable",
		"500 Internal Server Error": "unavailable",
	}
	for status, want := range cases {
		err := errors.New(`request GET https://example.invalid/x?token=secret-77 failed(status="` + status + `"): body secret-77`)
		if got := codeOf(err); got != want {
			t.Errorf("%s: %s, not %s", status, got, want)
		}
		if d := statusOf(err); d != "HTTP "+status[:3] {
			t.Errorf("%s: the detail is %q", status, d)
		}
	}
	if codeOf(errors.New("dial tcp: refused")) != "unavailable" || statusOf(errors.New("x")) != "no answer" {
		t.Error("an error with no status")
	}
}

func TestAJITConfigurationBecomesFiles(t *testing.T) {
	inner := base64.StdEncoding.EncodeToString([]byte(`{"agentName":"r"}`))
	enc := base64.StdEncoding.EncodeToString([]byte(`{".runner":"` + inner + `",".credentials":"{\"scheme\":\"OAuth\"}"}`))
	files, err := decodeJIT(enc)
	if err != nil || len(files) != 2 {
		t.Fatal(err, files)
	}
	if files[0]["name"] != ".credentials" || files[1]["content"] != `{"agentName":"r"}` {
		t.Errorf("%v", files)
	}
	if _, err := decodeJIT("not base64!"); err == nil {
		t.Error("a bad configuration is accepted")
	}
}

func TestTheEventsOfTheTierRule(t *testing.T) {
	for in, want := range map[string]string{"push": "push", "pull_request_target": "pull_request_target", "check_run": "other", "": "unknown"} {
		if got := eventWord(in); got != want {
			t.Errorf("%q: %q", in, got)
		}
	}
}

// Past the budget of the hour, a lookup is not made: the inputs are
// unknown, and the count of throttled lookups grows.
func TestTheTrustBudgetThrottles(t *testing.T) {
	tr := &trust{api: "http://127.0.0.1:1", budget: 1, signer: newSigner(nil, "1")}
	tr.lookup("o", "r", 1, "push")
	in, ok := tr.lookup("o", "r", 2, "push")
	if ok || in.actorClass != "unknown" || tr.Throttled != 1 {
		t.Fatalf("%v %v %d", in, ok, tr.Throttled)
	}
}

// The design test 4 of the source contract: after a revocation the
// sidecar ends with 78 (the class Revoked) and names no secret.
func TestARevocationEndsTheSidecarWith78(t *testing.T) {
	calls := 0
	handle := func(line []byte) map[string]any {
		calls++
		if calls == 2 {
			return errorReply("source-revoked", "the installation token was refused")
		}
		return map[string]any{"reply": "done"}
	}
	in := strings.NewReader("{\"schema\":1,\"op\":\"ack\",\"job_id\":\"j-1\"}\n{\"schema\":1,\"op\":\"ack\",\"job_id\":\"j-2\"}\n{\"schema\":1,\"op\":\"ack\",\"job_id\":\"j-3\"}\n")
	var out bytes.Buffer
	if code := serve(handle, in, &out); code != 78 {
		t.Fatalf("exit %d, want 78", code)
	}
	if calls != 2 {
		t.Fatalf("%d calls, want 2: the third line must not be read", calls)
	}
	if !strings.Contains(out.String(), "source-revoked") {
		t.Fatalf("no revoked reply: %q", out.String())
	}
}

// T-167: the repository of a run gives `visibility: null` and `private`.
func TestTheVisibilityOfAMinimalRepository(t *testing.T) {
	cases := map[string]string{
		`{"private":true,"visibility":null}`:       "private",
		`{"private":false,"visibility":null}`:      "public",
		`{"private":true}`:                         "private",
		`{"private":true,"visibility":"internal"}`: "internal",
		`{"private":false,"visibility":"public"}`:  "public",
		`{"visibility":null}`:                      "unknown",
		`{"private":"yes"}`:                        "unknown",
	}
	for in, want := range cases {
		var rp map[string]any
		if err := json.Unmarshal([]byte(in), &rp); err != nil {
			t.Fatal(err)
		}
		if got := visibilityOf(rp); got != want {
			t.Errorf("%s: %s, want %s", in, got, want)
		}
	}
}
