package main

import (
	"bufio"
	"context"
	"encoding/json"
	"fmt"
	"os"
	"sync"
	"time"
)

// signer asks sign, the module of podrunner, for an App JWT: one JSON line
// with the claims out on the descriptor 3, one line back with the token or
// an error. The key never reaches this process.
type signer struct {
	// sem is the lock of the channel: a wait for it ends with its context.
	semOnce sync.Once
	sem     chan struct{}
	f       *os.File
	// One goroutine reads the reply lines, so that a wait can end with its
	// context. stale counts the replies of requests whose wait ended: sign
	// answers in order, so they come first and are dropped.
	readOnce sync.Once
	lines    chan []byte
	stale    int
	iss      string
	cached   string
	until    time.Time
	// The fine-grained token of the organization (T-174).
	patToken string
}

// signWait: the longest wait for sign, which answers at once on the host.
var signWait = 5 * time.Second

func newSigner(f *os.File, iss string) *signer {
	return &signer{f: f, iss: iss}
}

func (s *signer) lock(ctx context.Context) error {
	s.semOnce.Do(func() { s.sem = make(chan struct{}, 1) })
	// select picks at random when both are ready.
	if err := ctx.Err(); err != nil {
		return fmt.Errorf("the sign channel: %w", err)
	}
	select {
	case s.sem <- struct{}{}:
		return nil
	case <-ctx.Done():
		return fmt.Errorf("the sign channel: %w", ctx.Err())
	}
}

func (s *signer) unlock() { <-s.sem }

// ask sends one request line and gives its reply line, within ctx and at
// most signWait. The caller holds the lock.
func (s *signer) ask(ctx context.Context, req []byte) ([]byte, error) {
	if s.f == nil {
		return nil, fmt.Errorf("no sign channel")
	}
	s.readOnce.Do(func() {
		s.lines = make(chan []byte, 16)
		go s.read()
	})
	ctx, cancel := context.WithTimeout(ctx, signWait)
	defer cancel()
	if _, err := s.f.Write(req); err != nil {
		return nil, fmt.Errorf("the sign channel: %w", err)
	}
	for {
		select {
		case line, ok := <-s.lines:
			if !ok {
				return nil, fmt.Errorf("the sign channel: closed")
			}
			if s.stale > 0 {
				s.stale--
				continue
			}
			return line, nil
		case <-ctx.Done():
			// Its reply comes later, before the reply of the next request.
			s.stale++
			return nil, fmt.Errorf("the sign channel: %w", ctx.Err())
		}
	}
}

func (s *signer) read() {
	r := bufio.NewReader(s.f)
	for {
		line, err := r.ReadBytes('\n')
		if err != nil {
			close(s.lines)
			return
		}
		s.lines <- line
	}
}

// token gives a JWT; sign signs at most once a minute, so a token is kept
// for most of its life of ten minutes.
func (s *signer) token(ctx context.Context) (string, error) {
	if err := s.lock(ctx); err != nil {
		return "", err
	}
	defer s.unlock()
	if s.cached != "" && time.Until(s.until) > time.Minute {
		return s.cached, nil
	}
	now := time.Now().Unix()
	req, _ := json.Marshal(map[string]any{"op": "sign", "claims": map[string]any{"iat": now - 30, "exp": now + 540, "iss": s.iss}})
	line, err := s.ask(ctx, append(req, '\n'))
	if err != nil {
		return "", err
	}
	var rep struct {
		Token string `json:"token"`
		Exp   int64  `json:"exp"`
		Error string `json:"error"`
	}
	if err := json.Unmarshal(line, &rep); err != nil || rep.Token == "" {
		return "", fmt.Errorf("sign refused: %s", rep.Error)
	}
	// sign can give its last token again, with an older exp (T-165).
	exp := now + 540
	if rep.Exp > 0 && rep.Exp < exp {
		exp = rep.Exp
	}
	s.cached, s.until = rep.Token, time.Unix(exp, 0)
	return s.cached, nil
}

// farFuture keeps a test token valid.
var farFuture = time.Now().Add(24 * time.Hour)

// pat asks podrunner for the fine-grained token of the organization once
// (T-174): `{"op":"pat"}` out on descriptor 3, `{"token":...}` back.
func (s *signer) pat(ctx context.Context) (string, error) {
	if err := s.lock(ctx); err != nil {
		return "", err
	}
	defer s.unlock()
	if s.patToken != "" {
		return s.patToken, nil
	}
	line, err := s.ask(ctx, []byte("{\"op\":\"pat\"}\n"))
	if err != nil {
		return "", err
	}
	var rep struct {
		Token string `json:"token"`
	}
	if err := json.Unmarshal(line, &rep); err != nil || rep.Token == "" {
		return "", fmt.Errorf("the token of the organization: none")
	}
	s.patToken = rep.Token
	return s.patToken, nil
}
