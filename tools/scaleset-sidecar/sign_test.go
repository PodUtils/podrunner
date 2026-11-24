package main

import (
	"bufio"
	"context"
	"fmt"
	"os"
	"syscall"
	"testing"
	"time"
)

// T-165: sign gives its last token again with an older exp; the sidecar
// keeps it only until that exp.
func TestATokenGivenAgainKeepsItsOwnExp(t *testing.T) {
	fds, err := syscall.Socketpair(syscall.AF_UNIX, syscall.SOCK_STREAM, 0)
	if err != nil {
		t.Fatal(err)
	}
	ours, theirs := os.NewFile(uintptr(fds[0]), "ours"), os.NewFile(uintptr(fds[1]), "theirs")
	defer ours.Close()
	defer theirs.Close()
	exp := time.Now().Unix() + 200
	go func() {
		r := bufio.NewReader(theirs)
		if _, err := r.ReadBytes('\n'); err == nil {
			fmt.Fprintf(theirs, "{\"token\":\"a.b.c\",\"exp\":%d}\n", exp)
		}
	}()
	s := newSigner(ours, "1")
	tok, err := s.token(1)
	if err != nil || tok != "a.b.c" {
		t.Fatal(tok, err)
	}
	if !s.until.Equal(time.Unix(exp, 0)) {
		t.Fatalf("kept until %v, want %v", s.until, time.Unix(exp, 0))
	}
}

// T-174: the token of the organization comes once from descriptor 3, and
// the trust lookups use it.
func TestTheTokenOfTheOrganizationComesOnce(t *testing.T) {
	fds, err := syscall.Socketpair(syscall.AF_UNIX, syscall.SOCK_STREAM, 0)
	if err != nil {
		t.Fatal(err)
	}
	ours, theirs := os.NewFile(uintptr(fds[0]), "ours"), os.NewFile(uintptr(fds[1]), "theirs")
	defer ours.Close()
	defer theirs.Close()
	asked := make(chan string, 4)
	go func() {
		r := bufio.NewReader(theirs)
		for {
			line, err := r.ReadBytes('\n')
			if err != nil {
				return
			}
			asked <- string(line)
			fmt.Fprintln(theirs, `{"token":"made-up-pat-77"}`)
		}
	}()
	s := newSigner(ours, "")
	tr := &trust{signer: s, pat: true}
	for i := 0; i < 2; i++ {
		tok, err := tr.installationToken(context.Background())
		if err != nil || tok != "made-up-pat-77" {
			t.Fatal(tok, err)
		}
	}
	if got := <-asked; got != "{\"op\":\"pat\"}\n" {
		t.Fatalf("asked %q", got)
	}
	select {
	case again := <-asked:
		t.Fatalf("asked again: %q", again)
	default:
	}
}
