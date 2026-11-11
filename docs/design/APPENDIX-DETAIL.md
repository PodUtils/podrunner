# Appendix: design detail moved out of SPEC.md

Text that `SPEC.md` summarises and points to, kept here so the SPEC stays under its word ceiling. Labels are those of the SPEC.

## Egress per unit kind (SPEC.md §9.1)

- `microvm`: no NIC. A vsock channel bridged to the broker, a loopback proxy in the guest. Hold `enforced` when the broker is the only channel. Documented for the archive launcher, shell-script job only: a guest with no NIC ran `apk add` through the broker (R5:8.3).
- `podvm` userland with `podnet pod`: INET stream and datagram sockets redirect to a unix-socket upstream (Documented, R2:1.2). Hold `advisory`: it passes unix, vsock and netlink sockets and host loopback TCP (Observed, R2:1.2 items 1, 2). Not a boundary against hostile code.
- `podvm` userland with proxy variables: `http_proxy` to the broker with the job token as password. Hold `none`: Node `fetch` and many tools ignore `HTTPS_PROXY` (Documented, plan §3). The token is then an environment variable of the unit, the one named exception to "no credential in the environment".

## The shim when a step kills it (SPEC.md §10.4)

If the shim is not alive at `destroy`, the driver cannot prove emptiness: it kills the processes that carry the marker (a step can clear the marker in a child, so this is best effort), reports `residual_processes` as `unknown`, and `slots` treats `unknown` as above 0. The death of the shim before the top process ends is `unknown_end` in `wait`. Leak checks match by the marker, not by argv word: on the loader family argv is `ld.so --library-path ... --argv0 sleep PATH N`.

## The podvm driver: cost of a store, and `container_exec` (SPEC.md §10.1)

The cost of a per-unit `PODVM_STORE` is one extraction for each unit (about 5 s for the runner image, M2) and one image copy of disk for each slot. Unverified: whether hardlinking an image between stores is safe. WP-V5 removes the cost, not the capability.

`container_exec` maps to `podvm exec -e K=V -w DIR` (both Native, Source, `parity.rs:289-290`). `stdin: true` is refused (`exec -i` is a no-op), stdout and stderr arrive as one stream, and a workdir inside a volume is refused (`parity.rs:222`, WP-V6).
