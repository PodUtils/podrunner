#!/usr/bin/env python3
"""Reference implementation of the tier rule of SPEC section 6.3.

compute_tier decides the tier of a job from trust_inputs and the configuration.
intake_verdict compares it with the tier of the set that delivered the job.

The rule is default deny and has two positive branches:
  trusted    every condition of TRUSTED_CONDITIONS holds;
  untrusted  the event is in UNTRUSTED_EVENTS and head_is_fork is true, that is
             a fork pull request (GitHub withholds secrets from it).
Every other job has no tier: compute_tier returns None and the intake verdict
is event-not-allowed, whatever the actor or the set. That includes every
unknown value, a same-repository pull request by an actor who is not a member,
a public repository not listed in trusted_public, and the events in
REFUSED_EVENTS. The tier local is never computed. A set whose tier is local
accepts only jobs that compute to trusted.

The named parts are compared by tools/check_tiers.py with the machine-readable
blocks of SPEC.md section 6.3:
  TRUSTED_EVENTS, UNTRUSTED_EVENTS, REFUSED_EVENTS  with the tier-rule block;
  the keys of TRUSTED_CONDITIONS and UNTRUSTED_CONDITIONS  with the condition
  names of the tier-prose block.
"""

TRUSTED_EVENTS = ("push", "pull_request", "workflow_dispatch", "schedule", "merge_group")
UNTRUSTED_EVENTS = ("pull_request",)
REFUSED_EVENTS = (
    "pull_request_target",
    "workflow_run",
    "issue_comment",
    "release",
    "repository_dispatch",
    "other",
    "unknown",
)
RANK = {"untrusted": 0, "trusted": 1, "local": 1}


def _visible(ti: dict, cfg: dict) -> bool:
    repo_full = f"{ti['owner']}/{ti['repo']}"
    return ti["repo_visibility"] in ("private", "internal") or repo_full in cfg.get("trusted_public", [])


TRUSTED_CONDITIONS = {
    "visible": _visible,
    "owner": lambda ti, cfg: ti["owner"] in cfg.get("organizations", []),
    "not_fork": lambda ti, cfg: ti["head_is_fork"] is False,
    "member": lambda ti, cfg: ti["actor_class"] == "member",
    "event": lambda ti, cfg: ti["event"] in TRUSTED_EVENTS,
}

UNTRUSTED_CONDITIONS = {
    "event": lambda ti, cfg: ti["event"] in UNTRUSTED_EVENTS,
    "fork": lambda ti, cfg: ti["head_is_fork"] is True,
}


def compute_tier(ti: dict, cfg: dict):
    """trusted, untrusted, or None when the job has no tier (event-not-allowed)."""
    if all(check(ti, cfg) for check in TRUSTED_CONDITIONS.values()):
        return "trusted"
    if all(check(ti, cfg) for check in UNTRUSTED_CONDITIONS.values()):
        return "untrusted"
    return None


def intake_verdict(computed, set_tier: str) -> str:
    """accept, or the refusal code tier-mismatch or event-not-allowed."""
    if computed is None:
        return "event-not-allowed"
    return "accept" if RANK[computed] >= RANK[set_tier] else "tier-mismatch"
