# Summary for the owner

Date: 2026-11-10. Labels: Observed (run or read this session), Documented (a document says it), Inferred, Unverified. Unlabelled text is a design decision of the spec.

## What the plan is
`podrunner` is one static program that finds a CI job, picks the safest isolation the host offers, runs the official `actions/runner` inside it, and refuses what the host cannot isolate. The design has two seams (a job source and a unit driver) and two capability axes (container route, egress hold). The core knows no forge, no podvm, no .NET by name. Start with `SPEC.md`. The proposed new body of #47 is `ISSUE-47-REPLACEMENT.md`. Nothing is posted. Both archive bundles (`runner-trust`, `ghactions-local-phase0`) are credited and used as designs and tests, not as dependencies.

## What is wrong with #47
Observed: 29 corrections with ids C1 to C30, one id unused and 25 single points of failure (`APPENDIX-CORRECTIONS.md`, `APPENDIX-SPOF.md`). The heaviest:
- `pod-dockerapi` is not "the one missing piece". Container hooks come first and the Docker API waits for a verb census.
- `pod-guest` is not a microVM (C1). `-v` is Degraded, not unsupported (C2). `pod-testkit` holds no fake GitHub (C9). `podci` already exists, empty (C24).
- The runner credential cannot be "read once". It is three files the runner keeps (G-03 is restated, not met).
- Observed (M1, M2): `RLIMIT_FSIZE` kills the .NET runner at start. The unmodified runner does not start on podvm's userland rung (exit 154, then 137).
- Trust must come from job metadata. A fork job can name the trusted runner set.
- "Can start now" is false: the owner's 2026-11-10 ruling says leave #47 alone.

## What must change
Seams instead of fixed choices; secret holder split from job parsers; a scale set (outbound only) to find jobs; tiers `untrusted`, `trusted`, `local` computed by intake; honest host matrix; offline acceptance in three levels. Eighteen items are cut or deferred, each with a reopening trigger (SPEC section 15).

## Decisions owed
Six need an answer, eleven default (SPEC 2.1, 17 requests in all):
- DR-1, DR-2: name this plan and let `podrunner` get code. Without them nothing starts.
- DR-4: accept `redirect` isolation for `trusted` jobs on no-root hosts. Without it those hosts run no job.
- DR-5: own microVM driver now, or wait for podvm T-1656.
- DR-6: githome and cfb accept restated G-02, G-03, G-09.
- DR-7: a throwaway GitHub organization for live tests.

## What can start now
Nothing in podcrates or podrunner. podvm's own entries (T-1696, T-1695, T-1655) proceed in podvm's order, behind about 26 open entries (Documented). Inside this directory, `./verify.sh` runs. Milestone M-A (level 1 on fakes, podvm driver with a stand-in payload) needs no account and no second machine once DR-1 and DR-2 are yes.

## Not verified
- Anything on live GitHub: hooks variable (U6), several labels per scale set (U7), App permissions (U8, U12). 11 unknowns wait for DR-7. No GitHub account was used.
- GitHub terms for a third-party runner were not read (U15, DR-16).
- M1 and M2 ran on one Debian 13 host, no job. Sizes of work packages are guesses.
- The hold `advisory` of `podnet pod` has never run with the runner (U9).

## Package
The zip carries the whole tree. After unpacking, run `bash verify.sh`. Its manifest check compares every file with `MANIFEST.sha256`.
