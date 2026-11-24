#!/bin/sh
# The job-started hook of the runner image (T-175, T-493). GitHub binds no
# runner of a just-in-time configuration to one job, so this runner can get
# another job than the one that podrunner placed in this unit. podrunner
# writes the placed job, "OWNER/REPO RUN_ID", into the runner root
# (.podrunner-job). A job of the same run has the same trust inputs, so the
# run is enough. For a job of another run, the hook asks serve through the
# proxy of the unit, or else through the egress socket that
# PODRUNNER_EGRESS names (a podvm unit, T-530), at control.podrunner.test
# (the front of the egress socket): serve asks the source, checks the trust facts of the real job
# against the tier of the unit, and answers keep or end. No answer, no
# proxy or no node ends the job (fail closed). With no file (the local
# mode), there is nothing to compare.
# A fixed PATH and no options of node from the environment of the job.
PATH=/usr/bin:/bin
export PATH
unset NODE_OPTIONS NODE_PATH
case $0 in
  */*) root="${0%/*}/.." ;;
  *) root=.. ;;
esac
want_file="$root/.podrunner-job"
[ -f "$want_file" ] || exit 0
want=$(tr 'A-Z' 'a-z' < "$want_file")
got=$(printf '%s %s\n' "$GITHUB_REPOSITORY" "$GITHUB_RUN_ID" | tr 'A-Z' 'a-z')
[ "$got" = "$want" ] && exit 0

# The question: the facts of this job, as JSON, to serve. Exit 0 is keep;
# else the line on stdout is the code.
ask='
const http = require("http"), fs = require("fs");
const e = process.env;
let head_repository, head_is_fork;
// A fork is a head that is not the base: not head.repo.fork, as a pull
// request can come from the root of the fork network.
try {
  const pr = JSON.parse(fs.readFileSync(e.GITHUB_EVENT_PATH, "utf8")).pull_request;
  const h = pr && pr.head && pr.head.repo, b = pr && pr.base && pr.base.repo;
  if (h) head_repository = h.full_name;
  if (h && b && typeof h.id === "number" && typeof b.id === "number") head_is_fork = h.id !== b.id;
  else if (h && b && typeof h.full_name === "string" && typeof b.full_name === "string" && h.full_name && b.full_name)
    head_is_fork = h.full_name.toLowerCase() !== b.full_name.toLowerCase();
} catch (_) {}
const body = JSON.stringify({
  run_id: Number(e.GITHUB_RUN_ID), job_name: e.GITHUB_JOB || "", event: e.GITHUB_EVENT_NAME || "",
  repository: e.GITHUB_REPOSITORY || "", head_repository, head_is_fork, actor: e.GITHUB_ACTOR || ""
});
const end = (code) => { console.log(code); process.exit(3); };
// The proxy of the unit, else the egress socket of PODRUNNER_EGRESS.
let to;
const named = e.HTTP_PROXY || e.http_proxy || e.HTTPS_PROXY || e.https_proxy;
const sock = /^http:\/\/unix:(\/.+)$/.exec(e.PODRUNNER_EGRESS || "");
if (named) {
  try { const u = new URL(named); to = { host: u.hostname, port: u.port || 80 }; } catch (_) { end("no-proxy"); }
} else if (sock) {
  to = { socketPath: sock[1] };
} else {
  end("no-proxy");
}
const req = http.request({
  ...to, method: "POST", timeout: 15000,
  path: "http://control.podrunner.test/v1/assigned",
  headers: { "Host": "control.podrunner.test", "Content-Type": "application/json", "Content-Length": Buffer.byteLength(body) }
}, (res) => {
  let d = "";
  res.on("data", (c) => { d += c; });
  res.on("end", () => {
    let v;
    try { v = JSON.parse(d); } catch (_) { end("bad-answer"); }
    if (v.verdict === "keep") process.exit(0);
    end(String(v.code || "end").replace(/[^a-z0-9-]/g, "").slice(0, 64));
  });
});
req.on("timeout", () => { req.destroy(); end("source-unavailable"); });
req.on("error", () => end("no-answer"));
req.end(body);
'
node=""
for n in "$root/externals/node24/bin/node" "$root/externals/node20/bin/node"; do
  if [ -x "$n" ]; then node=$n; break; fi
done
code="no-node"
if [ -n "$node" ]; then
  code=$("$node" -e "$ask" 2>/dev/null) && exit 0
fi
# An annotation names the cause in the summary of the job, not only in
# the log of the hook. It names this job, not the placed one (T-201).
msg="podrunner: this runner was placed for another job: the job ends before its first step (this job: $got; $code)"
echo "::error title=podrunner::$msg"
echo "$msg" >&2
# The runner exits 0 after this job, so the host reads this line in the log
# of the unit (the stderr of Runner.Listener, an ancestor) to report the
# job as job-mismatch (T-552). The name of the line is HOOK_ENDED of
# crates/podrunner-slots/src/log_tail.rs.
pid=$$
while [ "$pid" -gt 1 ] 2>/dev/null; do
  comm=$(cat "/proc/$pid/comm" 2>/dev/null) || break
  if [ "$comm" = Runner.Listener ]; then
    { printf 'podrunner-job-started: ended: %s\n' "$code" >> "/proc/$pid/fd/2"; } 2>/dev/null
    break
  fi
  pid=$(sed 's/.*) . \([0-9]*\) .*/\1/' "/proc/$pid/stat" 2>/dev/null) || break
done
exit 1
