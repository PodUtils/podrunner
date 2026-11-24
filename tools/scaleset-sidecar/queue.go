package main

import (
	"context"
	"fmt"
	"strconv"
	"strings"
	"time"

	"github.com/actions/scaleset"
)

// The queue holds the jobs of the messages until a `next` has capacity for
// them: a message can hold the jobs of a large matrix, and a lookup is made
// only for a job that a `next` takes. GitHub deleted each message before
// its jobs came here, and the queue is in memory only: the end of the
// sidecar names the jobs that it held (closeSession).
//
// A job with no final answer keeps its place, and the jobs behind it are
// still offered: the order of the offers binds no job to a runner, as
// GitHub gives a runner of the scale set any queued job of the set
// (T-175). Its run waits for a retry with no call; past attemptMax calls
// with no final answer, or pendingMax, the job is offered with unknown
// trust inputs, as for a final answer with no facts: a trusted set refuses
// it, and the sidecar cancels its run (Q7).
var (
	retryFirst  = 5 * time.Second
	retryMax    = time.Minute
	attemptMax  = 10
	pendingMax  = 65 * time.Minute
	statusEvery = time.Minute
)

// queued is a job that waits for capacity.
type queued struct {
	id   string
	job  *job
	base scaleset.JobMessageBase
	// since: when the job came. pending: the first `next` with no final
	// answer for it; attempts: the calls of its run with no final answer;
	// why: the last cause.
	since, pending time.Time
	attempts       int
	why            string
}

// retryWait: the time of the next call for a run with no final answer.
type retryWait struct {
	at   time.Time
	step time.Duration
}

// next gives the events of the messages that the poller found, and at
// most `capacity` offers. It waits at most `wait` (T-164); the lookups of
// the offers have the time nextBudget.
func (s *sidecar) next(capacity int, wait time.Duration) map[string]any {
	events := []any{}
	msgs, err := s.poll.take(capacity, wait)
	if err != nil && len(s.queue) == 0 {
		return errorReply(codeOf(err), statusOf(err))
	}
	for _, msg := range msgs {
		if msg.Statistics != nil {
			s.stats = msg.Statistics
		}
		for _, a := range msg.JobAvailableMessages {
			s.enqueue(&a.JobMessageBase, false)
		}
		for _, a := range msg.JobAssignedMessages {
			s.enqueue(&a.JobMessageBase, true)
		}
		for _, st := range msg.JobStartedMessages {
			events = append(events, map[string]any{"event": "started", "job_id": s.idOf(&st.JobMessageBase), "runner_name": st.RunnerName})
		}
		for _, c := range msg.JobCompletedMessages {
			if s.dropQueued(&c.JobMessageBase) {
				continue // podrunner never knew it
			}
			id := s.idOf(&c.JobMessageBase)
			delete(s.byJobID, c.JobID)
			if c.Result == "canceled" || c.Result == "cancelled" {
				events = append(events, map[string]any{"event": "cancel", "job_id": id})
			} else {
				events = append(events, map[string]any{"event": "completed", "job_id": id, "result": c.Result})
			}
		}
	}
	gaveUp := s.GivenUp
	// One time limit for the cancels and the lookups: the cancels take at
	// most half of it, so a slow endpoint does not stop the offers. While
	// retries are due, the offers can get only the other half.
	ctx, cancel := context.WithTimeout(s.ctx, nextBudget)
	defer cancel()
	retry, stop := context.WithTimeout(ctx, nextBudget/2)
	s.retryCancels(retry)
	stop()
	events = append(events, s.offerQueued(ctx, capacity)...)
	s.keepHeld()
	s.report(s.GivenUp != gaveUp)
	return map[string]any{"reply": "events", "events": events}
}

// offerQueued offers at most `capacity` queued jobs, in the order of the
// queue, in the time of ctx.
func (s *sidecar) offerQueued(ctx context.Context, capacity int) []any {
	events := []any{}
	if capacity <= 0 || len(s.queue) == 0 {
		return events
	}
	b := newBatch(ctx)
	rest := make([]queued, 0, len(s.queue))
	runs := map[int64]bool{}
	for i := range s.queue {
		q := &s.queue[i]
		var offer map[string]any
		if capacity > 0 {
			offer = s.offerOf(b, q)
		}
		if offer == nil {
			rest = append(rest, *q)
			runs[q.base.WorkflowRunID] = true
			continue
		}
		s.jobs[q.id] = q.job
		events = append(events, map[string]any{"event": "offer", "offer": offer})
		capacity--
	}
	s.queue = rest
	for run := range s.waits {
		if !runs[run] {
			delete(s.waits, run)
		}
	}
	return events
}

// offerOf gives the offer of a queued job, or nil when it waits.
func (s *sidecar) offerOf(b *batch, q *queued) map[string]any {
	a := &q.base
	run, now := a.WorkflowRunID, time.Now()
	// The retry of a run waits for the next `next`, not for the other
	// jobs of the run in this one: they share its result.
	if _, done := b.runs[run]; !done && s.waits[run] != nil && now.Before(s.waits[run].at) {
		return s.pastLimit(q, now)
	}
	in, whole, st, why := s.trust.lookupIn(b, a.OwnerName, a.RepositoryName, run, a.EventName, q.since)
	switch st {
	case final:
		delete(s.waits, run)
		if !whole {
			s.TrustLookupFailed++
		}
		q.job.attempt = in.attempt
		s.dropOlderCancels(run, in.attempt)
		return s.offerWith(q, in)
	case notReached:
		return s.pastLimit(q, now)
	case retry:
		q.attempts++
		s.backoff(run, now)
	case throttled:
		// No attempt, but no call on each tick: a conditional read is a
		// call too.
		s.backoff(run, now)
	}
	if q.pending.IsZero() {
		q.pending = now
	}
	q.why = why
	return s.pastLimit(q, now)
}

// backoff sets the next call of a run with no final answer, once in a
// `next`: the calls of a broken run do not take the time of each `next`
// (the engine calls `next` each second).
func (s *sidecar) backoff(run int64, now time.Time) {
	if s.waits == nil {
		s.waits = map[int64]*retryWait{}
	}
	w := s.waits[run]
	if w == nil {
		w = &retryWait{}
		s.waits[run] = w
	}
	if w.at.After(now) {
		return
	}
	w.step = min(max(w.step*2, retryFirst), retryMax)
	w.at = now.Add(w.step)
}

// pastLimit gives the offer with unknown inputs of a job past the limits,
// and names it on stderr; else nil.
func (s *sidecar) pastLimit(q *queued, now time.Time) map[string]any {
	if q.pending.IsZero() || (q.attempts < attemptMax && now.Sub(q.pending) < pendingMax) {
		return nil
	}
	a := &q.base
	s.GivenUp++
	logf("job %s of the run %d of %s/%s: the trust lookup gave no final answer in %d attempts over %s (%s): "+
		"it is offered with unknown trust inputs", q.id, a.WorkflowRunID, a.OwnerName, a.RepositoryName,
		q.attempts, now.Sub(q.pending).Round(time.Second), q.why)
	return s.offerWith(q, trustInputs{event: eventWord(a.EventName), headIsFork: "unknown", actorClass: "unknown", visibility: "unknown"})
}

// offerWith builds job-offer.v1 from the message of a job and its trust
// inputs. It has no tier and no payload.
func (s *sidecar) offerWith(q *queued, t trustInputs) map[string]any {
	a := &q.base
	labels := a.RequestLabels
	if labels == nil {
		labels = []string{}
	}
	return map[string]any{
		"schema": 1, "job_id": q.id, "source": s.o.source, "labels": labels, "arch": s.o.arch,
		"wait_deadline_s": s.o.waitDeadlineS,
		"trust_inputs": map[string]any{
			"event": t.event, "head_is_fork": t.headIsFork, "actor_class": t.actorClass,
			"repo_visibility": t.visibility, "owner": a.OwnerName, "repo": a.RepositoryName,
			"run_id": a.WorkflowRunID, "ref": "", "workflow_ref": a.JobWorkflowRef,
		},
	}
}

// idOf gives the job id of the wire of a message: the id of its offer
// when its jobId is known, else its runner request, else `a-JOBID`.
func (s *sidecar) idOf(b *scaleset.JobMessageBase) string {
	if id, ok := s.byJobID[b.JobID]; ok && b.JobID != "" {
		return id
	}
	if b.RunnerRequestID != 0 {
		return strconv.FormatInt(b.RunnerRequestID, 10)
	}
	return "a-" + b.JobID
}

// enqueue queues the offer of a JobAvailable or a JobAssigned message.
// A job that is known by its jobId (a redelivery, or the JobAssigned of
// a job that this sidecar acquired) is not offered again.
func (s *sidecar) enqueue(b *scaleset.JobMessageBase, assigned bool) {
	if b.JobID != "" && s.byJobID[b.JobID] != "" {
		return
	}
	id := s.idOf(b)
	if assigned && b.RunnerRequestID == 0 && (b.JobID == "" || !jobIDPattern.MatchString(id)) {
		return // no id that the wire can carry
	}
	if s.jobs[id] != nil {
		return
	}
	for _, q := range s.queue {
		if q.id == id {
			return
		}
	}
	if b.JobID != "" {
		s.byJobID[b.JobID] = id
	}
	now := time.Now()
	j := &job{request: b.RunnerRequestID, assigned: assigned && b.RunnerRequestID == 0, state: offered,
		owner: b.OwnerName, repo: b.RepositoryName, run: b.WorkflowRunID, since: now}
	s.queue = append(s.queue, queued{id: id, job: j, base: *b, since: now})
}

// dropQueued removes the queued job of a JobCompleted message: GitHub
// cancelled it, or another runner took it. True when it was queued.
func (s *sidecar) dropQueued(b *scaleset.JobMessageBase) bool {
	for i, q := range s.queue {
		if (b.JobID != "" && q.base.JobID == b.JobID) || (b.RunnerRequestID != 0 && q.job.request == b.RunnerRequestID) {
			s.queue = append(s.queue[:i], s.queue[i+1:]...)
			if q.base.JobID != "" {
				delete(s.byJobID, q.base.JobID)
			}
			return true
		}
	}
	return false
}

// report writes the counts of the queue on stderr: at once with force,
// else at most each statusEvery while a job has no final answer or a
// count changed.
func (s *sidecar) report(force bool) {
	now := time.Now()
	pending := 0
	var oldest time.Duration
	for _, q := range s.queue {
		if !q.pending.IsZero() {
			pending++
		}
		oldest = max(oldest, now.Sub(q.since))
	}
	counts := [3]int{s.trust.Throttled, s.TrustLookupFailed, s.GivenUp}
	if !force && ((pending == 0 && counts == s.lastCounts) || now.Sub(s.lastReport) < statusEvery) {
		return
	}
	s.lastReport, s.lastCounts = now, counts
	line := fmt.Sprintf("the queue holds %d jobs (the oldest for %s), %d of them with no final answer; "+
		"throttled %d, failed %d, given up %d", len(s.queue), oldest.Round(time.Second), pending,
		s.trust.Throttled, s.TrustLookupFailed, s.GivenUp)
	if st := s.stats; st != nil {
		line += fmt.Sprintf("; GitHub counts %d available and %d assigned jobs", st.TotalAvailableJobs, st.TotalAssignedJobs)
	}
	logf("%s", line)
}

// keepHeld keeps the names of the queued jobs for closeSession, which can
// run in the goroutine of a signal.
func (s *sidecar) keepHeld() {
	held := make([]string, 0, len(s.queue))
	for _, q := range s.queue {
		held = append(held, fmt.Sprintf("%s (run %d of %s/%s)", q.id, q.base.WorkflowRunID, q.base.OwnerName, q.base.RepositoryName))
	}
	s.heldMu.Lock()
	s.held = held
	s.heldMu.Unlock()
}

// heldLine names the queued jobs at the end of the sidecar, or "".
func (s *sidecar) heldLine() string {
	s.heldMu.Lock()
	defer s.heldMu.Unlock()
	if len(s.held) == 0 {
		return ""
	}
	names := s.held
	if len(names) > 50 {
		names = append(names[:50:50], fmt.Sprintf("and %d more", len(s.held)-50))
	}
	return fmt.Sprintf("%d queued jobs end with the sidecar; GitHub deleted their messages, and it is not known "+
		"that it sends them again: %s", len(s.held), strings.Join(names, ", "))
}

// sessionStartLine names the jobs that GitHub counts for the set at the
// start of a session, when there are some: this sidecar holds no job, and
// they can be the jobs of an earlier sidecar.
func sessionStartLine(st *scaleset.RunnerScaleSetStatistic) string {
	if st == nil || st.TotalAvailableJobs+st.TotalAcquiredJobs+st.TotalAssignedJobs == 0 {
		return ""
	}
	return fmt.Sprintf("the session starts; GitHub counts %d available, %d acquired and %d assigned jobs of the set, "+
		"and this sidecar holds no job: a job of an earlier sidecar whose message GitHub deleted comes again only "+
		"if GitHub sends it again", st.TotalAvailableJobs, st.TotalAcquiredJobs, st.TotalAssignedJobs)
}
