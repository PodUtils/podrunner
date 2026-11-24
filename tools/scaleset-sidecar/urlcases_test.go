package main

import (
	"encoding/json"
	"os"
	"strings"
	"testing"
)

// urlCase is one row of tools/config-url-cases.json, the table that the
// Rust check reads too: the two cannot drift.
type urlCase struct {
	URL     string `json:"url"`
	Refuse  string `json:"refuse"`
	Parts   int    `json:"parts"`
	API     string `json:"api"`
	GHESAPI string `json:"ghes_api"`
}

func urlCases(t *testing.T) []urlCase {
	t.Helper()
	b, err := os.ReadFile("../config-url-cases.json")
	if err != nil {
		t.Fatal(err)
	}
	var v struct {
		Cases []urlCase `json:"cases"`
	}
	if err := json.Unmarshal(b, &v); err != nil || len(v.Cases) == 0 {
		t.Fatalf("the table: %v", err)
	}
	return v.Cases
}

// refusal gives the class of a refusal of config_url.
func refusal(err error) string {
	switch {
	case err == nil:
		return ""
	case strings.Contains(err.Error(), "no query"):
		return "query"
	case strings.Contains(err.Error(), "no .git"):
		return "git"
	case strings.Contains(err.Error(), "is not the URL"):
		return "not"
	}
	return "other: " + err.Error()
}

// F of the second T-504 review: urlParts and apiURLOf agree with the
// shared table, with and without GITHUB_ACTIONS_FORCE_GHES.
func TestTheSharedTableOfConfigURLs(t *testing.T) {
	cases := urlCases(t)
	for _, c := range cases {
		parts, err := urlParts(c.URL)
		if got := refusal(err); got != c.Refuse {
			t.Errorf("%q: refusal %q, want %q", c.URL, got, c.Refuse)
			continue
		}
		if c.Refuse != "" {
			continue
		}
		if len(parts) != c.Parts {
			t.Errorf("%q: %d parts, want %d", c.URL, len(parts), c.Parts)
		}
		if got := apiURLOf(c.URL); got != c.API {
			t.Errorf("%q: the API %s, want %s", c.URL, got, c.API)
		}
	}
	// The sidecar reads the variable from its own environment, as
	// actions/scaleset does: set, also empty, is GHES.
	for _, v := range []string{"1", ""} {
		t.Setenv("GITHUB_ACTIONS_FORCE_GHES", v)
		for _, c := range cases {
			if c.Refuse != "" {
				continue
			}
			if got := apiURLOf(c.URL); got != c.GHESAPI {
				t.Errorf("GHES %q: %q: the API %s, want %s", v, c.URL, got, c.GHESAPI)
			}
			if tr := newTrust(options{configURL: c.URL}, nil); tr.api != c.GHESAPI {
				t.Errorf("GHES %q: newTrust of %q: %s", v, c.URL, tr.api)
			}
		}
	}
}
