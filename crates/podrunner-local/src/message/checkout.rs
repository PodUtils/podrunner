//! The stand-in of `actions/checkout` in the local mode (T-092, T-133,
//! T-425).

use pod_actions_workflow::{Kind, Node, Step as WorkflowStep};

use std::collections::BTreeMap;

/// The script of the stand-in for the step `s`, and what it checks out.
pub(super) fn stand_in(
    s: &WorkflowStep,
    pins: &BTreeMap<String, String>,
    self_repo: &str,
) -> Result<(String, String), String> {
    let line = s.mark.line;
    let with = |name: &str| {
        s.with.as_ref().and_then(|w| match &w.kind {
            Kind::Map(e) => e.iter().find(|(k, _)| k.name == name).map(|(_, v)| v),
            _ => None,
        })
    };
    let path = match with("path") {
        None => String::new(),
        Some(Node { kind: Kind::String(p), .. }) if !p.contains("${{") && !p.split('/').any(|c| c == "..") => p.clone(),
        Some(_) => return Err(format!("line {line}: the `path` of the local checkout must be a plain relative path")),
    };
    let level = match with("submodules").map(|n| &n.kind) {
        None | Some(Kind::Bool(false)) => 0,
        Some(Kind::Bool(true)) => 1,
        Some(Kind::String(v)) if v == "false" => 0,
        Some(Kind::String(v)) if v == "true" => 1,
        Some(Kind::String(v)) if v == "recursive" => crate::tree::SUBMODULE_LEVELS,
        Some(_) => {
            return Err(format!(
                "line {line}: the `submodules` of the local checkout must be true, false or recursive"
            ));
        }
    };
    let target = crate::store::checkout_target(s.with.as_ref(), self_repo).map_err(|e| format!("line {line}: {e}"))?;
    match &target {
        None => Ok((checkout_script(&path, None, level), "the local tree".to_string())),
        Some(t) if level > 0 => Err(format!(
            "line {line}: the checkout of `{t}` with `submodules`: the store keeps no submodule of another repository"
        )),
        Some(t) => {
            let sha = pins.get(t).ok_or_else(|| format!("line {line}: the checkout of `{t}` is not pinned"))?;
            let repo = t.split('@').next().unwrap_or(t);
            Ok((checkout_script(&path, Some((repo, sha)), 0), format!("{t} at {sha}")))
        }
    }
}

/// The stand-in of `actions/checkout` in the local mode (T-092): the
/// tarball of the local tree from the fake broker (`github.podrunner_broker` names
/// it) into the workspace, or into `path` in it. No `tar -C` and no
/// `tar -z`: in a podvm unit GNU `tar -C DIR` failed to open an existing
/// DIR (T-093).
/// With `other` (`OWNER/REPO` and its commit, T-133), the tarball of that
/// commit from the store, with its top directory stripped. The local tree
/// keeps the submodules to the depth `level` (T-425).
fn checkout_script(path: &str, other: Option<(&str, &str)>, level: u32) -> String {
    let dest = if path.is_empty() { "$GITHUB_WORKSPACE".to_string() } else { format!("$GITHUB_WORKSPACE/{path}") };
    let (route, strip) = match other {
        None => (podrunner_broker_fake::actions::CHECKOUT.to_string(), ""),
        Some((repo, sha)) => {
            (format!("_actions/{}", podrunner_broker_fake::actions::archive_name(repo, sha)), " --strip-components=1")
        }
    };
    let script = CHECKOUT_SCRIPT.replace("DEST", &dest).replace("ROUTE", &route).replace("STRIP", strip);
    match other {
        None => script + &SUBMODULES.replace("LIST", crate::tree::SUBMODULES).replace("LEVEL", &level.to_string()),
        Some(_) => script,
    }
}

/// The script of [`checkout_script`]. The tree comes from the broker with
/// the job token in no argv (T-406): curl reads it on its standard input;
/// node (of the runner, in `/__e`, or of the image) and python3 read it
/// from their environment, so a job container with no curl
/// (`ubuntu:24.04`, `debian:*-slim`) gets its tree too (T-422). The tree
/// goes to a file first: a failed fetch fails the step.
pub(super) const CHECKOUT_SCRIPT: &str = r#"set -eu
mkdir -p "DEST"
cd "DEST"
u="${{ github.podrunner_broker }}ROUTE"
t="${{ github.podrunner_token }}"
get() {
  if command -v curl >/dev/null 2>&1; then
    printf 'header = "Authorization: Bearer %s"
' "$t" | curl -fsSL -K - "$u"
    return
  fi
  for n in /__e/node*/bin/node node; do
    if command -v "$n" >/dev/null 2>&1; then
      PODRUNNER_T="$t" PODRUNNER_U="$u" "$n" -e 'const u=process.env.PODRUNNER_U;require(u.startsWith("https")?"https":"http").get(u,{headers:{Authorization:"Bearer "+process.env.PODRUNNER_T}},r=>{if(r.statusCode!==200){console.error("podrunner: the tree: HTTP "+r.statusCode);process.exit(22)}r.pipe(process.stdout)}).on("error",e=>{console.error("podrunner: the tree: "+e.message);process.exit(7)})'
      return
    fi
  done
  if command -v python3 >/dev/null 2>&1; then
    PODRUNNER_T="$t" PODRUNNER_U="$u" python3 -c 'import os,sys,urllib.request as q;r=q.urlopen(q.Request(os.environ["PODRUNNER_U"],headers={"Authorization":"Bearer "+os.environ["PODRUNNER_T"]}));sys.stdout.buffer.write(r.read())'
    return
  fi
  echo "podrunner: the stand-in of actions/checkout needs curl, node or python3 in the job container" >&2
  return 1
}
f="${RUNNER_TEMP:-/tmp}/podrunner-tree-$$.tgz"
get > "$f"
gzip -dc < "$f" | tar -xf -STRIP
rm -f "$f""#;

/// The end of the stand-in for the local tree (T-425): each submodule one
/// level past LEVEL becomes an empty directory, as GitHub leaves it; a
/// submodule to LEVEL with no files fails the step with the fix. The list
/// goes.
const SUBMODULES: &str = r#"
m=LIST
if [ -f "$m" ]; then
  while read -r d r p; do
    if [ "$d" -le LEVEL ]; then
      if [ "$r" = 0 ]; then
        echo "podrunner: the submodule $p has no files in the local tree; fix: git submodule update --init --recursive" >&2
        rm -f "$m"
        exit 1
      fi
    elif [ "$d" -eq $((LEVEL + 1)) ]; then
      rm -rf "./$p"
      mkdir -p "./$p"
    fi
  done < "$m"
  rm -f "$m"
fi"#;
