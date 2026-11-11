// The scale-set sidecar of podrunner (GitHub #29): one source of jobs for
// one scale set (one tier, one architecture, one host). It speaks the
// source wire of podrunner on stdio (docs/contracts/source-driver.md) and
// the scale-set protocol of GitHub through actions/scaleset. It never
// holds the private key of the App: each App JWT comes from sign, the
// module of podrunner, over the descriptor 3 (one JSON line out, one
// line back; call C15).
//
//	podrunner-source-github-scaleset -config-url URL -api-url URL
//	    -installation-id N -scale-set NAME -source NAME -arch x86_64
//	    -app-id ID [-operator-controlled] [-wait-deadline-s 600]
package main

import (
	"bufio"
	"context"
	"encoding/json"
	"flag"
	"fmt"
	"io"
	"os"
	"strings"
)

// The longest line of the wire (docs/contracts/source-driver.md).
const lineMax = 1 << 20

type options struct {
	configURL, apiURL, scaleSet, source, arch, appID string
	installationID                                   int64
	operatorControlled                               bool
	waitDeadlineS, trustBudget                       int
}

func main() {
	var o options
	flag.StringVar(&o.configURL, "config-url", "", "the organization URL of the scale set")
	flag.StringVar(&o.apiURL, "api-url", "https://api.github.com", "the base URL of the GitHub REST API")
	flag.Int64Var(&o.installationID, "installation-id", 0, "the installation of the App")
	flag.StringVar(&o.scaleSet, "scale-set", "", "the name of the scale set")
	flag.StringVar(&o.source, "source", "", "the name of the source in podrunner.toml")
	flag.StringVar(&o.arch, "arch", "x86_64", "the architecture of the set")
	flag.StringVar(&o.appID, "app-id", "", "the id of the GitHub App: the iss of each JWT")
	flag.BoolVar(&o.operatorControlled, "operator-controlled", false, "the operator controls the source")
	flag.IntVar(&o.waitDeadlineS, "wait-deadline-s", 600, "the wait deadline of each offer")
	flag.IntVar(&o.trustBudget, "trust-budget", 1000, "the most trust lookups of one hour")
	flag.Parse()
	if o.configURL == "" || o.installationID == 0 || o.scaleSet == "" || o.source == "" {
		fmt.Fprintln(os.Stderr, "podrunner-source-github-scaleset: -config-url, -installation-id, -scale-set and -source are necessary")
		os.Exit(64)
	}
	signer := newSigner(os.NewFile(3, "sign"), o.appID)
	s, err := newSidecar(context.Background(), o, signer)
	if err != nil {
		fmt.Fprintln(os.Stderr, "podrunner-source-github-scaleset:", err)
		// A scope that cannot be served is a configuration: 78, no restart
		// helps (the table of pod-core).
		if strings.HasPrefix(err.Error(), "repo-scope-refused") {
			os.Exit(78)
		}
		os.Exit(69)
	}
	os.Exit(serve(s.handle, os.Stdin, os.Stdout))
}

// serve answers each request line of `in` with one reply line on `out`.
// After a `source-revoked` reply it ends with 78, the class Revoked of
// pod-core: no restart helps, and the operator must act.
func serve(handle func([]byte) map[string]any, in io.Reader, out io.Writer) int {
	r := bufio.NewReaderSize(in, 64*1024)
	w := bufio.NewWriter(out)
	for {
		line, err := readLine(r)
		if err == io.EOF {
			return 0
		}
		var rep map[string]any
		if err != nil {
			rep = errorReply("bad-request", err.Error())
		} else {
			rep = handle(line)
		}
		rep["schema"] = 1
		b, _ := json.Marshal(rep)
		_, _ = w.Write(append(b, '\n'))
		if err := w.Flush(); err != nil {
			return 74
		}
		if rep["code"] == "source-revoked" {
			return 78
		}
	}
}

// readLine reads one line of at most lineMax bytes. A longer line is
// read to its end and refused.
func readLine(r *bufio.Reader) ([]byte, error) {
	var buf []byte
	long := false
	for {
		chunk, err := r.ReadSlice('\n')
		if len(buf)+len(chunk) > lineMax+1 {
			long = true
		} else {
			buf = append(buf, chunk...)
		}
		if err == bufio.ErrBufferFull {
			continue
		}
		if err == io.EOF && len(buf) == 0 && !long {
			return nil, io.EOF
		}
		if long {
			return nil, fmt.Errorf("a line over %d bytes", lineMax)
		}
		return []byte(strings.TrimSuffix(string(buf), "\n")), nil
	}
}

func errorReply(code, detail string) map[string]any {
	return map[string]any{"reply": "error", "code": code, "detail": detail}
}
