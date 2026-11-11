#!/usr/bin/env python3
# Write the corpus of the fake broker (T-038) from the workflows of a
# podcrates checkout:
#
#   python3 -I scripts/corpus.py DIR > crates/podrunner-broker-fake/corpus/podcrates.json
#
# A job goes into the corpus when the fake broker can run it as GitHub
# would. Each other job goes into "excluded" with the reason. The fake does
# not evaluate expressions, has no actions other than the checkout, and
# gives no sudo. PyYAML reads the workflows.
import json
import os
import subprocess
import sys

import yaml

CHECKOUT = "actions/checkout"


def text(v):
    return json.dumps(v)


def reason(wf, job):
    if job.get("runs-on") != "ubuntu-24.04":
        return "runs-on is not ubuntu-24.04"
    for key in ("needs", "strategy", "services", "container", "environment"):
        if key in job:
            return f"the job has {key}"
    if "if" in job:
        return "the job has if"
    if "${{" in text(wf.get("env", {})) + text(job.get("env", {})):
        return "an env holds an expression"
    for step in job.get("steps", []):
        uses = step.get("uses", "")
        if uses and uses.split("@")[0] != CHECKOUT:
            return f"the step uses {uses.split('@')[0]}"
        run = step.get("run", "")
        if "sudo" in run.split():
            return "a step runs sudo"
        if "${{" in run + text(step.get("env", {})) + text(step.get("with", {})):
            return "a step holds an expression"
        for key in ("shell", "working-directory", "continue-on-error", "timeout-minutes"):
            if key in step:
                return f"a step has {key}"
    return None


def env(m):
    return [[k, str(v).lower() if isinstance(v, bool) else str(v)] for k, v in (m or {}).items()]


def job_of(name, wf, job):
    steps = []
    for step in job["steps"]:
        if "uses" in step:
            depth = (step.get("with") or {}).get("fetch-depth", 1)
            steps.append({"checkout": {"fetch_depth": int(depth)}})
            continue
        steps.append({
            "name": step.get("name", step["run"].splitlines()[0]),
            "run": step["run"],
            "if": str(step.get("if", "")),
            "env": env(step.get("env")),
        })
    return {"id": name, "env": env(wf.get("env")) + env(job.get("env")), "steps": steps}


def main():
    root = sys.argv[1]
    commit = subprocess.run(["git", "-C", root, "rev-parse", "HEAD"], check=True,
                            capture_output=True, text=True).stdout.strip()
    jobs, excluded = [], []
    wdir = os.path.join(root, ".github", "workflows")
    for f in sorted(os.listdir(wdir)):
        if not f.endswith(".yml"):
            continue
        with open(os.path.join(wdir, f)) as fh:
            wf = yaml.safe_load(fh)
        for jid, job in wf["jobs"].items():
            name = f"{f}/{jid}"
            why = reason(wf, job)
            if why:
                excluded.append({"id": name, "reason": why})
            else:
                jobs.append(job_of(name, wf, job))
    out = {"source": "PodUtils/podcrates", "commit": commit, "jobs": jobs, "excluded": excluded}
    json.dump(out, sys.stdout, indent=1)
    sys.stdout.write("\n")


if __name__ == "__main__":
    main()
