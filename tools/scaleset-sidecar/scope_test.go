package main

import (
	"fmt"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

// A fake REST API: example-org is an organization, each other owner is a
// personal account. The actor of run N is actors[N]. It counts the lookups
// of membership.
func scopeAPI(t *testing.T, members *int, actors map[string]string) *httptest.Server {
	ts := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch {
		case strings.HasSuffix(r.URL.Path, "/access_tokens"):
			w.WriteHeader(http.StatusCreated)
			_, _ = w.Write([]byte(`{"token":"t","expires_at":"2099-01-01T00:00:00Z"}`))
		case r.URL.Path == "/orgs/example-org":
			_, _ = w.Write([]byte(`{"login":"example-org"}`))
		case strings.Contains(r.URL.Path, "/members/"):
			*members++
			w.WriteHeader(http.StatusNoContent)
		case strings.Contains(r.URL.Path, "/actions/runs/"):
			actor := actors[r.URL.Path[strings.LastIndex(r.URL.Path, "/")+1:]]
			_, _ = w.Write([]byte(`{"event":"push","actor":{"login":"` + actor + `"},"repository":{"id":1,"private":true},"head_repository":{"id":1}}`))
		default:
			w.WriteHeader(http.StatusNotFound)
		}
	}))
	t.Cleanup(ts.Close)
	return ts
}

func scopeTrust(api string) *trust {
	return &trust{api: api, installation: 7, cache: map[int64]trustInputs{}, signer: &signer{cached: "a.b.c", until: farFuture}}
}

// Call C3 and T-504: the scope of the config URL. A personal account is
// refused unless the operator turns on allow_personal_account, and then
// only with the URL of one of its repositories. The organization scope
// does not change; the repository of an organization stays refused.
func TestTheScopeOfTheConfigURL(t *testing.T) {
	var n int
	ts := scopeAPI(t, &n, nil)
	cases := []struct {
		url      string
		personal bool
		// The prefix of the error, or "" for no error.
		want string
		// A word that the error must hold.
		word string
	}{
		{"https://github.com/example-org", false, "", ""},
		{"https://github.com/example-org/", true, "", ""},
		{"https://github.com/alice/app", false, "repo-scope-refused: alice is not an organization", "allow_personal_account = true"},
		{"https://github.com/alice", false, "repo-scope-refused: alice is not an organization", "allow_personal_account = true"},
		{"https://github.com/alice/app", true, "", ""},
		{"https://github.com/alice", true, "repo-scope-refused: alice is a personal account", "https://github.com/alice/REPO"},
		{"https://github.com/example-org/app", false, "repo-scope-refused: example-org/app is a repository of an organization", "config_url"},
		{"https://github.com/example-org/app", true, "repo-scope-refused: example-org/app is a repository of an organization", "config_url"},
		{"https://github.com/enterprises/e", true, "repo-scope-refused: ", "an organization"},
		{"https://github.com/alice/app/extra", true, "repo-scope-refused: ", "an organization"},
		{"https://github.com/", true, "repo-scope-refused: ", "an organization"},
	}
	for _, c := range cases {
		tr := scopeTrust(ts.URL)
		err := tr.scope(c.url, c.personal)
		if c.want == "" {
			if err != nil {
				t.Errorf("%s (personal %v): %v", c.url, c.personal, err)
			}
			continue
		}
		if err == nil || !strings.HasPrefix(err.Error(), c.want) || !strings.Contains(err.Error(), c.word) {
			t.Errorf("%s (personal %v): %v, want %q with %q", c.url, c.personal, err, c.want, c.word)
		}
		if tr.personal {
			t.Errorf("%s: a refused scope is personal", c.url)
		}
	}
	// A lookup that fails is no refusal: the sidecar starts again.
	tr := scopeTrust("http://127.0.0.1:1")
	if err := tr.scope("https://github.com/alice/app", true); err == nil || strings.HasPrefix(err.Error(), "repo-scope-refused") {
		t.Errorf("no answer: %v", err)
	}
}

// T-504: a personal account has no members. Only the account itself is
// `member`; each other actor (a collaborator, a bot) is `outside`, with no
// lookup of membership. An organization asks its membership as before.
func TestTheMemberOfAPersonalAccountIsTheAccount(t *testing.T) {
	var n int
	actors := map[string]string{"1": "alice", "2": "ALICE", "3": "bob", "4": "dependabot[bot]", "5": "mallory"}
	ts := scopeAPI(t, &n, actors)
	tr := scopeTrust(ts.URL)
	if err := tr.scope("https://github.com/Alice/app", true); err != nil || !tr.personal {
		t.Fatalf("%v %v", err, tr.personal)
	}
	want := map[int64]string{1: "member", 2: "member", 3: "outside", 4: "outside"}
	for run, w := range want {
		in, ok := tr.lookup("alice", "app", run, "push")
		if !ok || in.actorClass != w {
			t.Errorf("run %d (%s): %+v %v, want %s", run, actors[fmt.Sprint(run)], in, ok, w)
		}
	}
	// A job of another owner is never a member of this account, also when
	// its actor is that owner.
	if in, _ := tr.lookup("mallory", "app", 5, "push"); in.actorClass != "outside" {
		t.Errorf("another owner: %+v", in)
	}
	if n != 0 {
		t.Errorf("%d lookups of membership for a personal account", n)
	}
	org := scopeTrust(ts.URL)
	if err := org.scope("https://github.com/example-org", true); err != nil || org.personal {
		t.Fatalf("%v %v", err, org.personal)
	}
	if in, _ := org.lookup("example-org", "app", 3, "push"); in.actorClass != "member" || n != 1 {
		t.Errorf("an organization: %+v, %d lookups", in, n)
	}
}
