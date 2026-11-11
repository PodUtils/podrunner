#!/usr/bin/env python3
"""Mirror of the runner's response checks (HookResponse.cs, ContainerHookManager.cs:44-60,213-230), written by me from the source.
usage: check_hook_response.py COMMAND HAS_JOB_CONTAINER N_SERVICES RESPONSEFILE
Prints OK or the error the runner would raise. Inferred (not the runner's code): Newtonsoft matches property names case-insensitively."""
import json, sys
cmd, has_c, n_svc, path = sys.argv[1], sys.argv[2] == "1", int(sys.argv[3]), sys.argv[4]
raw = open(path).read().strip()
if not raw:
    if cmd == "prepare_job" and has_c:
        print("ERROR: Response file could not be read (required when a job container exists)"); sys.exit(1)
    print("OK (empty response)"); sys.exit(0)
r = {k.lower(): v for k, v in json.loads(raw).items()}
if cmd == "prepare_job":
    if has_c and r.get("isalpine") is None:
        print("ERROR: The property 'isAlpine' is required but was not found in the response file."); sys.exit(1)
    ctx = {k.lower(): v for k, v in (r.get("context") or {}).items()}
    svcs = ctx.get("services", [])
    if not isinstance(svcs, list):
        print("ERROR: context.services must be a JSON array (IList<ResponseContainer>), got %s" % type(svcs).__name__); sys.exit(1)
    if len(svcs) < n_svc:
        print("ERROR: context.services has %d entries, runner indexes %d (ContainerHookManager.cs:~237-241)" % (len(svcs), n_svc)); sys.exit(1)
print("OK"); sys.exit(0)
