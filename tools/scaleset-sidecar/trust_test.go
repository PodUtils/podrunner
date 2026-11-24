package main

import (
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

// head_is_fork says whether the head of a run is another repository than
// the base: a pull request from the upstream root of a fork network has a
// head with fork=false, and it is still not the base.
func TestTheHeadIsAForkWhenItIsNotTheBase(t *testing.T) {
	cases := []struct{ name, run, want string }{
		{"a non-fork head that is not the base", `{"repository":{"id":2,"full_name":"org/b","fork":true,"private":true},"head_repository":{"id":1,"full_name":"up/u","fork":false}}`, "yes"},
		{"a fork head", `{"repository":{"id":2,"full_name":"org/b","private":true},"head_repository":{"id":3,"full_name":"x/b","fork":true}}`, "yes"},
		{"the base, which is a fork", `{"repository":{"id":2,"full_name":"org/b","fork":true,"private":true},"head_repository":{"id":2,"full_name":"org/b","fork":true}}`, "no"},
		{"the same repository", `{"repository":{"id":2,"full_name":"org/b","private":true},"head_repository":{"id":2,"full_name":"org/b","fork":false}}`, "no"},
		{"no ids: the names in another case", `{"repository":{"full_name":"Org/B","private":true},"head_repository":{"full_name":"org/b"}}`, "no"},
		{"no ids: other names", `{"repository":{"full_name":"org/b","private":true},"head_repository":{"full_name":"up/b","fork":false}}`, "yes"},
		{"one id only: the names", `{"repository":{"id":2,"full_name":"org/b","private":true},"head_repository":{"full_name":"org/b"}}`, "no"},
		{"no head repository", `{"repository":{"id":2,"full_name":"org/b","private":true}}`, "unknown"},
		{"a null head repository", `{"repository":{"id":2,"full_name":"org/b","private":true},"head_repository":null}`, "unknown"},
		{"no repository", `{"head_repository":{"id":2,"full_name":"org/b","fork":false}}`, "unknown"},
		{"neither an id nor a name", `{"repository":{"id":2,"private":true},"head_repository":{"fork":false}}`, "unknown"},
		{"an id of another type", `{"repository":{"id":"2","private":true},"head_repository":{"id":"2","fork":false}}`, "unknown"},
	}
	for _, c := range cases {
		run := c.run
		ts := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			switch {
			case strings.HasSuffix(r.URL.Path, "/access_tokens"):
				w.WriteHeader(http.StatusCreated)
				_, _ = w.Write([]byte(`{"token":"t","expires_at":"2099-01-01T00:00:00Z"}`))
			case strings.Contains(r.URL.Path, "/actions/runs/"):
				_, _ = w.Write([]byte(strings.Replace(run, "{", `{"event":"pull_request_target","actor":{"login":"m"},`, 1)))
			case strings.Contains(r.URL.Path, "/members/"):
				w.WriteHeader(http.StatusNoContent)
			default:
				w.WriteHeader(http.StatusNotFound)
			}
		}))
		tr := &trust{api: ts.URL, installation: 7, signer: &signer{cached: "a.b.c", until: farFuture}}
		in, _ := tr.lookup("org", "b", 5, "pull_request_target")
		ts.Close()
		if in.headIsFork != c.want {
			t.Errorf("%s: %s, want %s", c.name, in.headIsFork, c.want)
		}
	}
}
