# Roadmap

The milestones of the work record, in order (GitHub #1). Open work is an
entry in `TODO/`: this page names entries by id, under the milestone of
their row, and has no open checkbox. `cargo todo check` refuses an id
under another milestone, and a `- [ ]` item.

## M0: Decisions and bootstrap

The rulings DR-1 and DR-2 are yes (T-002, T-003). The questions DR-4,
DR-5 and DR-6 are with the owner (T-004, T-005, T-006); DR-7 waits for
the ruling "Test accounts" (T-007); the defaults of DR-3 and DR-8 to
DR-17 are calls (T-008). The registers: T-009, T-010. The bootstrap:
T-011 (workspace), T-012 (gate), T-013 (frame), T-014 (spec). Exit: the
gate is green on each push. T-001 tracks the plan.

## M-A: Level 1 and the podvm unit

T-015 (beta release), T-016 (seams), T-017 (schemas), T-018 (tier rule),
T-019 (security documents), T-020 (fakes), T-021 (slots core), T-022
(CLI), T-023 (podvm driver), T-024 (egress). Exit (T-025): level 1 is
green, and the podvm driver passes the unit conformance with a stand-in
payload. No account, no second machine, no official runner.

## M-B: The official runner under podvm

T-026: podvm starts the official runner on its userland rung; podrunner
proves it through its driver, with no GitHub.

## M-C: The mint and the live gates

T-027 (mint), T-028 (fake scale set), T-029 (scale-set sidecar), T-030
(image pipeline). Exit (T-032): the live gates G-H2 to G-H4 on a `fake`
unit. It needs DR-7.

## M-D: One live trusted job

T-031 (remote mint). Exit (T-033): one live trusted job on an H0 host.
It needs DR-4 and M-B.

## L2: Level 2

T-034 (U19 and U21), T-035 (hook relay), T-036 (guest self-test), T-037
(microVM unit), T-038 (fake broker). Exit (T-039): level 2 is green.

## L3: Live acceptance

T-040. It needs DR-7.

## later: The crate dependencies later than v1

T-041 (hold `enforced`), T-042 (`confined`), T-043 (later crates).
