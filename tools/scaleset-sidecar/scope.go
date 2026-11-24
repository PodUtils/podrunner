package main

import (
	"fmt"
	"net/http"
	"net/url"
	"os"
	"strings"
)

// The refusal of a personal account with the opt-in off (call C3, T-504).
const personalOff = "repo-scope-refused: %s is not an organization (a personal account needs Administration: write; " +
	"to serve it, set allow_personal_account = true in its [[source]])"

// scope checks the config URL before the sidecar asks GitHub for a scale
// set (call C3): the URL of an organization; with personal (T-504), also
// the URL of a repository of a personal account. A repository of an
// organization is refused: its scale set needs Administration: write.
func (t *trust) scope(configURL string, personal bool) error {
	parts, err := urlParts(configURL)
	if err != nil {
		return err
	}
	owner := parts[0]
	typ, login, id, err := t.owner(owner)
	switch {
	case err != nil:
		return err
	case typ == "Organization" && len(parts) == 2:
		return fmt.Errorf("repo-scope-refused: %s/%s is a repository of an organization: give config_url the URL "+
			"of the organization (a repository scale set needs Administration: write)", owner, parts[1])
	case typ == "Organization":
		return nil
	case typ != "User":
		return fmt.Errorf("repo-scope-refused: the owner %s of config_url is a %q, not an organization or a user", owner, typ)
	case !personal:
		return fmt.Errorf(personalOff, owner)
	case len(parts) == 1:
		return fmt.Errorf("repo-scope-refused: %s is a personal account: give config_url the URL of one of its "+
			"repositories (https://github.com/%s/REPO)", owner, owner)
	}
	t.personal, t.account, t.accountID = true, login, id
	return nil
}

// urlParts gives the owner and the repository of a config URL, in the
// forms that actions/scaleset serves. A query, a fragment or a user is
// refused: it passes a split of the path, and the scale set then fails at
// each start (69) in place of one refusal (78).
func urlParts(configURL string) ([]string, error) {
	u, err := url.Parse(strings.Trim(configURL, "/"))
	if err != nil || (u.Scheme != "https" && u.Scheme != "http") || u.Host == "" {
		return nil, fmt.Errorf("repo-scope-refused: %q is not the URL of an organization or of a repository", configURL)
	}
	if u.RawQuery != "" || u.ForceQuery || u.Fragment != "" || strings.Contains(configURL, "#") || u.User != nil {
		return nil, fmt.Errorf("repo-scope-refused: %q: give config_url with no query, no fragment and no user", configURL)
	}
	parts := strings.Split(strings.Trim(u.EscapedPath(), "/"), "/")
	if len(parts) > 2 || asciiFold(parts[0], "enterprises") {
		parts = nil
	}
	for _, p := range parts {
		if !nameOK(p) {
			parts = nil
		}
	}
	if len(parts) == 0 {
		return nil, fmt.Errorf("repo-scope-refused: %q is not the URL of an organization or of a repository", configURL)
	}
	if len(parts) == 2 && len(parts[1]) > 4 && asciiFold(parts[1][len(parts[1])-4:], ".git") {
		return nil, fmt.Errorf("repo-scope-refused: %q: give config_url the URL of the repository with no .git", configURL)
	}
	return parts, nil
}

// nameOK: a login or a repository name of GitHub. Each other byte (an
// escape, a space, an expression) is no name.
func nameOK(s string) bool {
	if s == "" || s == "." || s == ".." {
		return false
	}
	for i := 0; i < len(s); i++ {
		c := s[i]
		if !(c >= 'a' && c <= 'z' || c >= 'A' && c <= 'Z' || c >= '0' && c <= '9' || c == '-' || c == '_' || c == '.') {
			return false
		}
	}
	return true
}

// asciiFold compares two names of GitHub. Not strings.EqualFold: a
// Unicode fold takes the Kelvin sign for a k.
func asciiFold(a, b string) bool {
	if len(a) != len(b) {
		return false
	}
	for i := 0; i < len(a); i++ {
		x, y := a[i], b[i]
		if 'A' <= x && x <= 'Z' {
			x += 'a' - 'A'
		}
		if 'A' <= y && y <= 'Z' {
			y += 'a' - 'A'
		}
		if x != y {
			return false
		}
	}
	return true
}

// owner gives the type of an owner ("User", "Organization"), its login
// and its id. Not /orgs: it answers 404 also for an owner that does not
// exist, which is a configuration, not a personal account.
func (t *trust) owner(owner string) (typ, login string, id int64, err error) {
	t.mu.Lock()
	defer t.mu.Unlock()
	code, v := t.get("/users/" + url.PathEscape(owner))
	switch {
	case code == http.StatusOK && v != nil:
		typ, _ = v["type"].(string)
		login, _ = v["login"].(string)
		n, _ := v["id"].(float64)
		if login == "" || !asciiFold(login, owner) {
			login = owner
		}
		return typ, login, int64(n), nil
	case code == http.StatusNotFound:
		return "", "", 0, fmt.Errorf("repo-scope-refused: the owner %s of config_url does not exist on GitHub (HTTP 404)", owner)
	default:
		return "", "", 0, fmt.Errorf("the owner %s: HTTP %d", owner, code)
	}
}

// apiURLOf gives the REST API that actions/scaleset sends its calls to for
// a config URL (gitHubAPIURL of its config.go): api.HOST on github.com and
// on *.ghe.com, else HOST/api/v3 (GHES). The lookups go to the same API.
func apiURLOf(configURL string) string {
	u, err := url.Parse(strings.Trim(configURL, "/"))
	if err != nil {
		return "https://api.github.com"
	}
	_, ghes := os.LookupEnv("GITHUB_ACTIONS_FORCE_GHES")
	host := u.Host
	switch {
	case ghes:
		return u.Scheme + "://" + host + "/api/v3"
	case strings.EqualFold(host, "www.github.com"):
		return u.Scheme + "://api.github.com"
	case strings.EqualFold(host, "github.com"), strings.EqualFold(host, "github.localhost"), strings.HasSuffix(host, ".ghe.com"):
		return u.Scheme + "://api." + host
	default:
		return u.Scheme + "://" + host + "/api/v3"
	}
}
