# podrunner design review: corrected plan and practical spec

Date: 2026-11-10. Subject: PodUtils/podcrates#47 and `docs/research/ci/podrunner-plan.md`, with #35 (cfb asks), #36 (githome asks) and the owner archive `podrunner-ideas`.

Start with `SPEC.md`. It is standalone. Everything else is evidence or a contract that it points at.

This is the cleaned copy that podrunner keeps (GitHub #14, T-014). It leaves out the texts of the issues as read (`inputs/`; read the issues), the review rounds and the briefs of the agents (`review/`, `process/`), the proposed body of PodUtils/podcrates#47 with its edit trail and its disposition table (podcrates material), the recon report of the owner's private archive (R5) and the manifest. The host name of the measurement machine, a statement about an account and a default clone path are removed. The broker policy schema that `tools/check_schemas.py` reads is a copy of the podcrates schema in `schemas/external/`. The code of podrunner and `docs/contracts/` move on from this design; `docs/decisions.md` holds the rulings since.

| Path | What it is |
| --- | --- |
| `SUMMARY.md` | One page for the owner: the plan, what is wrong with #47, what must change, decisions owed, what can start now, what is not verified |
| `SPEC.md` | The corrected design: decisions, edits to #47, work plan, design, evidence, the deferred list |
| `APPENDIX-CORRECTIONS.md` | The 29 verified corrections to #47 and its plan (the ids `C1` to `C30`, `C13` is unused) |
| `APPENDIX-SPOF.md` | The 25 single points of failure that #47 builds in (the ids `Sn`) |
| `APPENDIX-EXISTING.md` | What already exists that #47 proposes to build, in long form |
| `APPENDIX-UNKNOWNS.md` | The unknowns table (U1 to U25) and what settles each |
| `APPENDIX-ASKS.md` | The asks of #35 and #36 against the spec, the level 1 guard list, level 2 detail |
| `APPENDIX-THREATS.md` | The threat model table |
| `APPENDIX-MEASUREMENTS.md` | What M1 and M2 measured, in detail |
| `APPENDIX-DEFERRED.md` | The design rules kept for the items cut from v1 |
| `APPENDIX-DETAIL.md` | Design detail that `SPEC.md` summarises, kept out of it for length |
| `APPENDIX-FLOWS.md` | The configuration example and three walk-throughs of a job with the message trace |
| `contracts/` | The unit driver, source driver and container hook contracts (version 1) |
| `schemas/`, `examples/` | JSON Schemas for the capability record, the job offer, the run report and its public subset, valid and invalid examples (`examples/invalid/EXPECT.json` names the reason each is invalid), and the tier cases |
| `recon/` | Five recon reports (R1 to R4, R6) |
| `measurements/` | Runs on the official runner and on podvm, with scripts and raw logs |
| `tools/`, `verify.sh` | `./verify.sh` runs seven checks (schemas, contracts, fields, codes, tiers, docs, selftest), then `MANIFEST.sha256` if present. It stops with exit 2 and a message if `jsonschema` is missing. `tools/selftest.sh` plants a defect for each guard in a temporary copy of the tree and checks that the real tree is unchanged. Needs `python3` with `jsonschema`, `bash`, `sha256sum`, `tar` |

The directory is relocatable: no absolute paths, every script resolves its place from its own location. `measurements/m2-podvm/scripts/00-build.sh` needs `PODVM_REPO`, a clone of podvm.

Labels in every document: Observed, Source, Documented, Inferred, Unverified. A statement with no label is a design decision.
