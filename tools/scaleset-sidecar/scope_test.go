package main

import (
	"context"
	"net/http"
	"strconv"
	"strings"
	"testing"
	"time"
)

func scopeTrust(api string) *trust {
	return &trust{api: api, installation: 7, signer: &signer{cached: "a.b.c", until: farFuture}}
}

// Call C3 and T-504: the scope of the config URL. A personal account is
// refused unless the operator turns on allow_personal_account, and then
// only with the URL of one of its repositories. The organization scope
// does not change; the repository of an organization stays refused. A URL
// that actions/scaleset cannot serve is refused before a lookup.
func TestTheScopeOfTheConfigURL(t *testing.T) {
	f, ts := newFakeGitHub(t)
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
		{"https://github.com/Alice/app.js", true, "", ""},
		{"https://github.com/alice", true, "repo-scope-refused: alice is a personal account", "https://github.com/alice/REPO"},
		{"https://github.com/example-org/app", false, "repo-scope-refused: example-org/app is a repository of an organization", "config_url"},
		{"https://github.com/example-org/app", true, "repo-scope-refused: example-org/app is a repository of an organization", "config_url"},
		{"https://github.com/enterprises/e", true, "repo-scope-refused: ", "an organization"},
		{"https://github.com/alice/app/extra", true, "repo-scope-refused: ", "an organization"},
		{"https://github.com/", true, "repo-scope-refused: ", "an organization"},
		// F3: an owner that does not exist is no personal account.
		{"https://github.com/ghost", false, "repo-scope-refused: the owner ghost of config_url does not exist", "HTTP 404"},
		{"https://github.com/ghost/app", true, "repo-scope-refused: the owner ghost of config_url does not exist", "HTTP 404"},
		// F4: the forms that pass a split of the path, and that no scale
		// set serves.
		{"https://github.com/alice/app.git", true, "repo-scope-refused: ", ".git"},
		{"https://github.com/alice/APP.GIT", true, "repo-scope-refused: ", ".git"},
		{"https://github.com/alice/app?tab=x", true, "repo-scope-refused: ", "no query"},
		{"https://github.com/alice/app?", true, "repo-scope-refused: ", "no query"},
		{"https://github.com/alice/app#readme", true, "repo-scope-refused: ", "no query"},
		{"https://github.com/example-org?x=1", false, "repo-scope-refused: ", "no query"},
		{"https://user@github.com/alice/app", true, "repo-scope-refused: ", "no query"},
		{"https://github.com/alice/..", true, "repo-scope-refused: ", "an organization"},
		{"https://github.com/alice/.", true, "repo-scope-refused: ", "an organization"},
		{"https://github.com/al%2Fice/app", true, "repo-scope-refused: ", "an organization"},
		{"https://github.com/alice/${{ github.repository }}", true, "repo-scope-refused: ", "an organization"},
		{"https://github.com/alice//app", true, "repo-scope-refused: ", "an organization"},
		{"ftp://github.com/alice/app", true, "repo-scope-refused: ", "an organization"},
		{"github.com/alice/app", true, "repo-scope-refused: ", "an organization"},
		{"", true, "repo-scope-refused: ", "an organization"},
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
	for _, status := range []int{http.StatusInternalServerError, http.StatusForbidden} {
		f.status = status
		for _, u := range []string{"https://github.com/alice/app", "https://github.com/example-org"} {
			err := scopeTrust(ts.URL).scope(u, true)
			if err == nil || strings.HasPrefix(err.Error(), "repo-scope-refused") || !strings.Contains(err.Error(), "HTTP") {
				t.Errorf("%s, HTTP %d: %v", u, status, err)
			}
		}
	}
}

// T-504: a personal account has no members. Only the account itself is
// `member`; each other actor (a collaborator, a bot) is `outside`, with no
// lookup of membership. An organization asks its membership as before.
func TestTheMemberOfAPersonalAccountIsTheAccount(t *testing.T) {
	f, ts := newFakeGitHub(t)
	actors := map[int64]string{1: "alice", 2: "ALICE", 3: "bob", 4: "dependabot[bot]", 5: "mallory"}
	for run, a := range actors {
		f.run(strconv.FormatInt(run, 10), runObject(user(a, 0), "", 1))
	}
	tr := scopeTrust(ts.URL)
	if err := tr.scope("https://github.com/Alice/app", true); err != nil || !tr.personal {
		t.Fatalf("%v %v", err, tr.personal)
	}
	want := map[int64]string{1: "member", 2: "member", 3: "outside", 4: "outside"}
	for run, w := range want {
		in, ok := tr.lookup("alice", "app", run, "push")
		if !ok || in.actorClass != w {
			t.Errorf("run %d (%s): %+v %v, want %s", run, actors[run], in, ok, w)
		}
	}
	// A job of another owner is never a member of this account, also when
	// its actor is that owner.
	if in, _ := tr.lookup("mallory", "app", 5, "push"); in.actorClass != "outside" {
		t.Errorf("another owner: %+v", in)
	}
	if f.memberLookups != 0 {
		t.Errorf("%d lookups of membership for a personal account", f.memberLookups)
	}
	f.members["bob"] = true
	org := scopeTrust(ts.URL)
	if err := org.scope("https://github.com/example-org", true); err != nil || org.personal {
		t.Fatalf("%v %v", err, org.personal)
	}
	if in, _ := org.lookup("example-org", "app", 3, "push"); in.actorClass != "member" || f.memberLookups != 1 {
		t.Errorf("an organization: %+v, %d lookups", in, f.memberLookups)
	}
}

// A lookup that GitHub does not answer ends at the time limit of a call,
// and its job stays queued: it is not refused (unknown) and not lost.
func TestTheLookupsHaveATimeout(t *testing.T) {
	f, ts := newFakeGitHub(t)
	f.run("1", runObject(user("alice", 1), "", 1))
	defer func(d time.Duration) { lookupTimeout = d }(lookupTimeout)
	lookupTimeout = 200 * time.Millisecond
	tr := newTrust(options{apiURL: ts.URL + "/", installationID: 7}, &signer{cached: "a.b.c", until: farFuture})
	f.setDelay(3 * time.Second)
	start := time.Now()
	_, _, st, why := tr.lookupIn(newBatch(context.Background()), "example-org", "app", 1, "push", time.Now())
	if took := time.Since(start); st != retry || why != "timeout" || took > time.Second {
		t.Fatalf("state %v (%s) after %v", st, why, took)
	}
	// The API of newTrust has no slash at its end (D of the review).
	if tr.api != ts.URL {
		t.Errorf("the API is %s", tr.api)
	}
	f.setDelay(0)
	if in, ok := tr.lookup("example-org", "app", 1, "push"); !ok || in.actorClass != "member" {
		t.Fatalf("%+v %v", in, ok)
	}
}

// F2 of the T-504 review: the lookups go to the API that actions/scaleset
// derives from the config URL (gitHubAPIURL of its config.go); the forms
// are in TestTheSharedTableOfConfigURLs.
func TestTheAPIOfTheConfigURL(t *testing.T) {
	if tr := newTrust(options{configURL: "https://www.github.com/org"}, nil); tr.api != "https://api.github.com" {
		t.Errorf("the default: %s", tr.api)
	}
	if tr := newTrust(options{configURL: "https://github.com/org", apiURL: "http://x"}, nil); tr.api != "http://x" {
		t.Errorf("a given API: %s", tr.api)
	}
}
