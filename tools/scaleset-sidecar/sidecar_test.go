package main

import (
	"encoding/base64"
	"errors"
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
