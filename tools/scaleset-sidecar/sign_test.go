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
	tok, err := s.token(context.Background())
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

// P3-8 of the third T-504 review: a wait for sign ends with its context.
// The late reply of that request is not the reply of the next one: sign
// answers in order, and the sidecar drops it.
func TestALateReplyOfSignIsNotTheNextReply(t *testing.T) {
	defer func(d time.Duration) { signWait = d }(signWait)
	signWait = 200 * time.Millisecond
	fds, err := syscall.Socketpair(syscall.AF_UNIX, syscall.SOCK_STREAM, 0)
	if err != nil {
		t.Fatal(err)
	}
	ours, theirs := os.NewFile(uintptr(fds[0]), "ours"), os.NewFile(uintptr(fds[1]), "theirs")
	defer ours.Close()
	defer theirs.Close()
	go func() {
		r := bufio.NewReader(theirs)
		for n := 1; ; n++ {
			if _, err := r.ReadBytes('\n'); err != nil {
				return
			}
			if n == 1 {
				time.Sleep(400 * time.Millisecond)
			}
			fmt.Fprintf(theirs, "{\"token\":\"t%d\",\"exp\":%d}\n", n, time.Now().Unix()+500)
		}
	}()
	s := newSigner(ours, "1")
	start := time.Now()
	if tok, err := s.token(context.Background()); err == nil || time.Since(start) > time.Second {
		t.Fatalf("the first: %q %v after %v", tok, err, time.Since(start))
	}
	// A context that has ended ends the wait for the lock too.
	done, cancel := context.WithCancel(context.Background())
	cancel()
	if _, err := s.token(done); err == nil {
		t.Fatal("an ended context got a token")
	}
	time.Sleep(300 * time.Millisecond)
	if tok, err := s.token(context.Background()); err != nil || tok != "t2" {
		t.Fatalf("the second: %q %v", tok, err)
	}
}
