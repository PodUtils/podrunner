# Appendix: walk-throughs of a job, and the configuration example

Design walk-throughs, not measurements. They show the order of the operations of `contracts/` and the refusal that each branch ends in. The state names are those of `SPEC.md` §11.1.

## Configuration example

The shape of `podrunner.toml` (`SPEC.md` §11.7). Key names are design, values are examples.

```toml
state_dir = "/var/lib/podrunner"        # journal, pin list, credential directories, per-unit stores
allow_fake = false

[[source]]
kind = "github-scaleset"                  # source id is kind/key
key = "org-a-trusted-x86_64"
tier = "trusted"                          # the tier of the set
arch = "x86_64"
slots = 1
labels = ["podrunner-trusted"]
wait_limit = 300                          # seconds, becomes wait_deadline_s
containers = false                        # true: the set serves jobs that may use containers
same_principal_ok = false                 # per source, never across tiers
affinity = "repository"                   # always on a shared principal, unless same_principal_ok = true
registries = ["ghcr.io"]                  # registry hosts allowed for job images, passed to the unit as `registries`
operator_controlled = false
payload_from_offer = false                # a fake source that states a payload in its offers is marked true

[unit.podvm]
driver = "podvm"
slots = 1                                 # the effective count on a shared principal is min(slots, 1)

[tier.trusted]
min_boundary = "confined"                 # "redirect" only after DR-4
organizations = ["org-a"]
trusted_public = []
slots_reserved = 1
[tier.untrusted]
min_boundary = "guest"                    # fixed, the file cannot lower it
slots_max = 2

[egress]
upstream = "http+exec:..."                # optional, SPEC §9.4
[pin]
max_age_days = 28
[deadlines]
prepare_s = 60
claim_s = 30
mint_s = 30
start_s = 120
running_s = 300
grace_s = 30
destroy_s = 60
report_s = 60
```

## Flow A: a trusted push on H0 (x86_64, no root, no mount, podvm userland rung)

Configuration: the example above. The minting side runs on another machine (`SPEC.md` §5.3), and this host reaches it over HTTPS with a host token.

Branch 1, the DR-4 default (`tier.trusted.min_boundary = "confined"`):

1. `hello` answers the instance and tier. `slots` runs `probe` on the podvm driver: `boundary: redirect`, `principal: shared`, `egress_hold: advisory`.
2. The tier minimum is `confined` and no configured kind reaches it. The host starts no source instance for the trusted tier. `status` shows `no-tier-served`. Nothing is polled and no job runs. This is the answer of the DR-4 default.

Branch 2, DR-4 answered yes (`tier.trusted.min_boundary = "redirect"`):

1. `next` with `capacity: 1` returns an `offer` event that `mint` assigned to this host from its ledger: `examples/valid/job-offer.trusted-push.json`. `slots` journals `offered`, then `ack` for the message.
2. Intake: the architecture matches. The tier computes to `trusted` (`tools/tier_rule.py`), equal to the set's tier. `mint` computed the same tier from the run object before it assigned the offer.
3. Placement (`SPEC.md` §6.5): the effective boundary is `redirect`. The podvm record satisfies the boundary. `limits_required: ["wall"]` needs `tree_kill: verified`, which the record does not give until the per-unit subreaper shim passes conformance test 3: until then `limit-not-enforceable`. With `tree_kill: verified` the flow continues. The hold needed is `advisory`, for an `allow` or an `open` policy.
4. The payload comes from the pin list entry of the set. The pin age is checked against the smaller of the pin's `max_age_days` and the offer's `image_max_age_days`. `prepare` for the pinned digest (cached). `claim`.
5. `slots` creates the credential directory (mode 0700) and calls `mint` with `runner_name` and `work_folder`. The credential comes back as three decoded files, once for this job. `slots` writes them with mode 0600.
6. `egress.load` binds the job token and policy to the unit's own listener. `start` with `credential_dir`, `runner_root`, `egress` and `control_socket`, and the start environment in `env`. The driver starts the unit's subreaper shim and `podvm run` with its own `PODVM_STORE`, and copies the files into the runner root. The payload starts with no credential in argv or environment. The state is `started`.
7. The job-started hook posts `job-started` to `POST /v1/job_state`, and `next` returns a `started` event whose `runner_name` and `job_id` match the claim. The state is `running`, and `claim_to_first_step_s` is measured. A `started` event with another runner name destroys the unit (`job-mismatch`).
8. The job ends. `wait` answers `done`. `destroy` walks the shim's descendants and answers residual counts 0. `egress.drop`. The report (`examples/valid/run-report.done.json`) is sent, with `same_principal_ok: false` and `credential_store: private-dir`. The state is `reported`.

A second trusted job arriving during step 7 waits: `no-slot-of-kind`, because `max_concurrent` is 1 on a `shared` principal.

## Flow B: a fork pull request on H2 (KVM, microvm)

1. `next` returns `examples/valid/job-offer.fork-pr.json` in the untrusted set. It states no `unit_need`: GitHub discovers container needs at hook time. The tier computes to `untrusted`, equal to the set's. A fork pull request that names the trusted set instead computes to `untrusted`, less than that set's tier: `tier-mismatch`, refused before the claim. The source gets `withdraw` with the code, and the job is suppressed for `terminal_suppress_s`.
2. Placement: the minimum is `guest`, fixed. The `microvm` record passed the start-up self-test of `SPEC.md` §6.6. The hold needed is `enforced`, and the record says so.
3. The `microvm` kind reports `container: none`, and a guest has no hook. The `untrusted` set is configured `containers = false`. A workflow that uses `container:` or `services:` and names this set fails inside the runner with the runner's own docker error. A source that states `unit_need.container: true` (the `fake` source does, `examples/valid/job-offer.fake-container.json`) is refused at placement with `container-not-supported`. A job with only `run:` steps proceeds.
4. `prepare`, `claim`, `mint`, `start` as in flow A. The credential directory is the guest's tmpfs (`credential_store: tmpfs`), the principal is `distinct`.
5. After the job, `destroy` removes the guest. A source that is not marked `operator_controlled` receives only the public report (`examples/valid/run-report-public.done.json`, `SPEC.md` §11.4).

## Flow C: a `pull_request_target` run

The run object says the event is `pull_request_target` and the actor is external. The tier rule gives no tier, and placement step 2 ends with `event-not-allowed`. The job is not acquired. The source gets `withdraw` with the code. `status` counts it. The same event from a member actor is refused too, and so is every job that is neither `trusted` nor a fork `pull_request`: a `release`, a same-repository pull request by an external actor, a push to a public repository that `trusted_public` does not list (`SPEC.md` §6.3).
