The crate dependencies later than v1 (GitHub #41 to #43).

# T-041: Hold enforced on podvm units (pod-hold or pod-sud) (GitHub #41)

**Source:** GitHub #41
**Category:** feature
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** blocked

## Problem

The text of GitHub #41 ("Hold `enforced` on podvm units (`pod-hold` or `pod-sud`)"), cleaned:

**What and why.** The hold says how the network traffic of a unit reaches the egress broker: `none`, `advisory` (a seccomp redirect that a payload can go around), or `enforced`. The `untrusted` tier needs `enforced`. A podvm unit on a no-root host reaches `advisory` at most today (through `podnet pod`). With an `enforced` hold on podvm units, podrunner can serve more than `trusted` jobs at `advisory` there, and the fixed denies of the broker hold.

**Dependencies in other repositories.**

- PodUtils/podcrates@T-109 (hold research) decides the crate (`pod-hold` or `pod-sud`). The hold reports a level and a proof. podcrates ask: PodUtils/podvm#227, section 2.7.
- podnet said in PodUtils/podnet#414 that the import of the generic hold comes after its backend split (PodUtils/podnet@T-040) and seccomp fixes (PodUtils/podnet@T-142 to PodUtils/podnet@T-158). Those entries look done. podnet ask: PodUtils/podvm#228, section 2.9.

**Content (podrunner side).** The podvm driver (#23) reports `egress_hold: enforced` with its proof when the crate gives one; placement (#21) and egress (#24) read it. No change to the seam contract (#16): `egress_hold` exists already.

**Acceptance.**

- PodUtils/podcrates@T-109 names the crate and its proof form.
- The podvm driver reports `enforced` with a proof only where the crate proves it, and a test plants a false claim that fails.

Later than v1: no milestone. Dependencies: PodUtils/podvm#227 section 2.7 (PodUtils/podcrates@T-109), PodUtils/podvm#228 section 2.9. Estimate (mine): S on the podrunner side.

## Premise

Read the text of GitHub #41 on 2026-11-11. Nothing measured.

## Approach

The podvm driver reports `egress_hold: enforced` with its proof when the crate gives one; a test plants a false claim.

## Prove

`cargo test -p podrunner-podvm hold`

## Blocker

podcrates names the crate of the hold and its proof (PodUtils/podcrates@T-109); podvm (on break) takes it.

# T-042: The confined boundary (pod-confine) (GitHub #42)

**Source:** GitHub #42
**Category:** feature
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** blocked

## Problem

The text of GitHub #42 ("`confined` boundary (`pod-confine`)"), cleaned:

**What and why.** The boundary order is `none` < `redirect` < `confined` < `guest`. `redirect` rewrites file paths and is not isolation against hostile code; every podvm rung is `redirect`. `confined` is a kernel-enforced boundary with no root (Landlock, seccomp, rlimits). With it, `trusted` jobs run on a no-root host without the owner's yes to DR-4 (#4).

**Dependencies in other repositories.** PodUtils/podcrates@T-108 (`pod-confine`) gives a capability record with `boundary: confined` and a proof string. podcrates ask: PodUtils/podvm#227, section 2.7.

**Content (podrunner side).** A unit kind or a podvm driver option that reports `boundary: confined` with the proof of PodUtils/podcrates@T-108. The capability schema (#17) and placement (#21) take `confined` already.

**Acceptance.**

- A unit reports `confined` only with a proof from `pod-confine`, and a planted false claim fails.
- With DR-4 answered no, a `trusted` job runs on a `confined` unit.

Later than v1: no milestone. Dependencies: PodUtils/podvm#227 section 2.7 (PodUtils/podcrates@T-108). Estimate (mine): -.

## Premise

Read the text of GitHub #42 on 2026-11-11. Nothing measured.

## Approach

A unit kind with `boundary: confined` and the proof of `pod-confine`.

## Prove

`cargo test -p podrunner-unit confined`

## Blocker

podcrates releases `pod-confine` (PodUtils/podcrates@T-108).

# T-043: Later crates: service unit, release index, acceptance registry (GitHub #43)

**Source:** GitHub #43
**Category:** feature
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** blocked

## Problem

The text of GitHub #43 ("Later crates: service unit, release index, acceptance registry"), cleaned:

**What and why.** Three podcrates entries serve podrunner after v1. None is on the path of v1.

- `pod-unit` (PodUtils/podcrates@T-102): a service unit for a hosted `podrunner` service. Until then `podrunner print-unit` (#22) writes the unit.
- `pod-release` (PodUtils/podcrates@T-104): signed release indexes (cfb ask A-X-13). Until then #15 publishes betas with SHA-256 sums and a build provenance attestation.
- `pod-todo` registry (PodUtils/podcrates@T-105): the "Proves" column as a registry of acceptance (cfb ask A-X-14). Until then the record of #13 maps each issue to an entry.

**Acceptance.**

- When each entry is released, an issue here takes it, or this issue records why not.

Later than v1: no milestone. Dependencies: PodUtils/podcrates@T-102, PodUtils/podcrates@T-104, PodUtils/podcrates@T-105 (entries exist; no new ask). Estimate (mine): -.

## Premise

Read the text of GitHub #43 on 2026-11-11. Nothing measured.

## Approach

Take each crate when podcrates releases it.

## Prove

`cargo test --workspace`

## Blocker

podcrates builds the crates (PodUtils/podvm#227, section 2.7); none is planned for v1.
