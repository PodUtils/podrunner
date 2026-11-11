package main

import (
	"bufio"
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
	mu     sync.Mutex
	f      *os.File
	r      *bufio.Reader
	iss    string
	cached string
	until  time.Time
}

func newSigner(f *os.File, iss string) *signer {
	s := &signer{f: f, iss: iss}
	if f != nil {
		s.r = bufio.NewReader(f)
	}
	return s
}

// token gives a JWT; sign signs at most once a minute, so a token is kept
// for most of its life of ten minutes.
func (s *signer) token(int64) (string, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.cached != "" && time.Until(s.until) > time.Minute {
		return s.cached, nil
	}
	if s.f == nil {
		return "", fmt.Errorf("no sign channel")
	}
	now := time.Now().Unix()
	req, _ := json.Marshal(map[string]any{"op": "sign", "claims": map[string]any{"iat": now - 30, "exp": now + 540, "iss": s.iss}})
	if _, err := s.f.Write(append(req, '\n')); err != nil {
		return "", fmt.Errorf("the sign channel: %w", err)
	}
	line, err := s.r.ReadBytes('\n')
	if err != nil {
		return "", fmt.Errorf("the sign channel: %w", err)
	}
	var rep struct {
		Token string `json:"token"`
		Error string `json:"error"`
	}
	if err := json.Unmarshal(line, &rep); err != nil || rep.Token == "" {
		return "", fmt.Errorf("sign refused: %s", rep.Error)
	}
	s.cached, s.until = rep.Token, time.Unix(now+540, 0)
	return s.cached, nil
}
