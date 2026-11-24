package main

import (
	"strconv"
	"testing"
)

// The T-504 review: the class of a run holds for its actor and for its
// triggering actor, and the stricter one wins. A re-run keeps the run id
// and the actor, and only its triggering actor tells who started it.
func TestTheTriggeringActorHasTheClassToo(t *testing.T) {
	f, ts := newFakeGitHub(t)
	f.run("1", runObject(user("alice", 1), user("bob", 7), 1))
	f.run("2", runObject(user("alice", 1), user("alice", 1), 1))
	f.run("3", runObject(user("alice", 1), "", 0))
	personal := scopeTrust(ts.URL)
	if err := personal.scope("https://github.com/alice/app", true); err != nil {
		t.Fatal(err)
	}
	for run, want := range map[int64]string{1: "outside", 2: "member", 3: "member"} {
		if in, _ := personal.lookup("alice", "app", run, "push"); in.actorClass != want {
			t.Errorf("personal, run %d: %+v, want %s", run, in, want)
		}
	}
	// An organization: an outside collaborator re-runs the run of a member.
	f.run("4", runObject(user("alice", 1), user("mallory", 3), 1))
	f.run("5", runObject(user("mallory", 3), user("alice", 1), 1))
	org := scopeTrust(ts.URL)
	if err := org.scope("https://github.com/example-org", false); err != nil {
		t.Fatal(err)
	}
	for run, want := range map[int64]string{2: "member", 4: "outside", 5: "outside"} {
		if in, _ := org.lookup("example-org", "app", run, "push"); in.actorClass != want {
			t.Errorf("organization, run %d: %+v, want %s", run, in, want)
		}
	}
	// One login is one lookup of membership, also for both fields.
	f.memberLookups = 0
	f.run("6", runObject(user("alice", 1), user("alice", 1), 1))
	if in, _ := org.lookup("example-org", "app", 6, "push"); in.actorClass != "member" || f.memberLookups != 1 {
		t.Errorf("run 6: %+v, %d lookups", in, f.memberLookups)
	}
}

// A re-run is a new attempt of the same run id: the inputs of the first
// attempt are not used for it.
func TestAReRunIsLookedUpAgain(t *testing.T) {
	f, ts := newFakeGitHub(t)
	for _, c := range []struct {
		url, owner string
		personal   bool
	}{{"https://github.com/alice/app", "alice", true}, {"https://github.com/example-org", "example-org", false}} {
		tr := scopeTrust(ts.URL)
		if err := tr.scope(c.url, c.personal); err != nil {
			t.Fatal(err)
		}
		f.run("9", runObject(user("alice", 1), user("alice", 1), 1))
		if in, ok := tr.lookup(c.owner, "app", 9, "push"); !ok || in.actorClass != "member" {
			t.Fatalf("%s, attempt 1: %+v %v", c.owner, in, ok)
		}
		f.run("9", runObject(user("alice", 1), user("mallory", 3), 2))
		if in, ok := tr.lookup(c.owner, "app", 9, "push"); !ok || in.actorClass != "outside" {
			t.Errorf("%s, attempt 2: %+v %v", c.owner, in, ok)
		}
		// The same attempt again comes from the cache: no new lookup of
		// membership.
		n := f.memberLookups
		if in, _ := tr.lookup(c.owner, "app", 9, "push"); in.actorClass != "outside" || f.memberLookups != n {
			t.Errorf("%s, attempt 2 again: %+v, %d lookups", c.owner, in, f.memberLookups-n)
		}
	}
}

// The account of a personal account is its id when the run gives ids, and
// else its login in an ASCII case fold: a Unicode fold takes the Kelvin
// sign for a k.
func TestTheAccountIsItsIdOrItsASCIILogin(t *testing.T) {
	f, ts := newFakeGitHub(t)
	tr := scopeTrust(ts.URL)
	if err := tr.scope("https://github.com/kate/app", true); err != nil {
		t.Fatal(err)
	}
	cases := []struct {
		actor, want string
	}{
		{user("kate", 2), "member"},
		{user("KATE", 0), "member"},
		{user("kate-renamed", 2), "member"},
		{user("kate", 99), "outside"},
		{user("\u212aate", 0), "outside"},
	}
	for i, c := range cases {
		f.run(strconv.Itoa(i+10), runObject(c.actor, "", 1))
		if in, _ := tr.lookup("kate", "app", int64(i+10), "push"); in.actorClass != c.want {
			t.Errorf("%s: %+v, want %s", c.actor, in, c.want)
		}
	}
}

// C of the second T-504 review: a re-run (attempt 2) with no triggering
// actor names no one who started it: the class is unknown, also when
// the actor is the account or a member.
func TestAReRunWithNoTriggeringActorIsUnknown(t *testing.T) {
	f, ts := newFakeGitHub(t)
	f.run("1", runObject(user("alice", 1), "", 2))
	f.run("2", runObject(user("alice", 1), "null", 2))
	f.run("3", runObject(user("alice", 1), "", 1))
	f.run("4", runObject(user("alice", 1), "", 0))
	for _, c := range []struct{ url, owner string }{{"https://github.com/alice/app", "alice"}, {"https://github.com/example-org", "example-org"}} {
		tr := scopeTrust(ts.URL)
		if err := tr.scope(c.url, c.owner == "alice"); err != nil {
			t.Fatal(err)
		}
		for run, want := range map[int64]string{1: "unknown", 2: "unknown", 3: "member", 4: "member"} {
			if in, _ := tr.lookup(c.owner, "app", run, "push"); in.actorClass != want {
				t.Errorf("%s, run %d: %+v, want %s", c.owner, run, in, want)
			}
		}
	}
}
