// The fake scale-set service of podrunner (GitHub #28): a test partner of
// the scale-set sidecar (GitHub #29) that speaks the protocol of the
// GitHub Actions runner scale sets, as the Go module actions/scaleset
// uses it. It is built in CI only and never released.
//
//	fake-scaleset -listen 127.0.0.1:0 -script FILE [-record FILE]
//
// It prints its base URL on the first line of stdout. The script gives the
// jobs; the record file gets one JSON line for each request.
package main

import (
	"flag"
	"fmt"
	"net"
	"net/http"
	"os"
)

func main() {
	listen := flag.String("listen", "127.0.0.1:0", "the address to listen on")
	script := flag.String("script", "", "the JSON script of the jobs")
	record := flag.String("record", "", "a file for one JSON line for each request")
	flag.Parse()
	sc, err := readScript(*script)
	if err != nil {
		fmt.Fprintln(os.Stderr, "fake-scaleset:", err)
		os.Exit(64)
	}
	l, err := net.Listen("tcp", *listen)
	if err != nil {
		fmt.Fprintln(os.Stderr, "fake-scaleset:", err)
		os.Exit(69)
	}
	base := "http://" + l.Addr().String()
	srv, err := newServer(base, sc, *record)
	if err != nil {
		fmt.Fprintln(os.Stderr, "fake-scaleset:", err)
		os.Exit(74)
	}
	fmt.Println(base)
	if err := http.Serve(l, srv); err != nil {
		fmt.Fprintln(os.Stderr, "fake-scaleset:", err)
		os.Exit(70)
	}
}
