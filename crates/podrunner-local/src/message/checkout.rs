//! The stand-in of `actions/checkout` in the local mode (T-092, T-133,
//! T-425).

use pod_actions_workflow::{Kind, Node, Step as WorkflowStep};

use std::collections::BTreeMap;

use crate::tree::refs::ask;

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
        None => same_repo(with("ref"), &path, level, pins, line),
        Some(t) if level > 0 => Err(format!(
            "line {line}: the checkout of `{t}` with `submodules`: the store keeps no submodule of another repository"
        )),
        Some(t) => {
            let sha = pins.get(t).ok_or_else(|| format!("line {line}: the checkout of `{t}` is not pinned"))?;
            let repo = t.split('@').next().unwrap_or(t);
            Ok((checkout_script(&path, Route::Other(repo, sha), 0), format!("{t} at {sha}")))
        }
    }
}

/// A checkout of the same repository: the local tree, or with `ref:` the
/// commit that the run resolved in the local git (T-640). A ref with no
/// commit fails the step with its reason, as GitHub fails it.
fn same_repo(
    node: Option<&Node>,
    path: &str,
    level: u32,
    pins: &BTreeMap<String, String>,
    line: u32,
) -> Result<(String, String), String> {
    use crate::tree::refs::{Ask, key};
    let tree = |what: String| Ok((checkout_script(path, Route::Tree, level), what));
    let r = match ask(node) {
        Ask::Default => return tree("the local tree".into()),
        Ask::Expression(e) => return tree(format!("the local tree; `ref: {e}` is an expression")),
        Ask::Bad(why) => return Ok((fail_script(&why), why)),
        Ask::Ref(r) => r,
    };
    match pins.get(&key(&r)).map(String::as_str) {
        Some("") => tree(format!("the local tree, the commit of `ref: {r}`")),
        Some(why) if why.starts_with('!') => Ok((fail_script(&why[1..]), why[1..].to_string())),
        Some(_) if level > 0 => Err(format!(
            "line {line}: the checkout of `ref: {r}` with `submodules`: the stand-in gives submodules only in the local tree"
        )),
        Some(sha) => Ok((checkout_script(path, Route::Commit(sha), 0), format!("ref {r} at {sha}"))),
        None => {
            let why = format!(
                "`ref: {r}`: the run resolved no commit for it; fix: give --checkout DIR, a git tree that has it"
            );
            Ok((fail_script(&why), why))
        }
    }
}

/// A step that fails with one line on stderr. The text goes in single
/// quotes; it holds no expression (an expression stays the local tree).
fn fail_script(why: &str) -> String {
    let why = why.replace('\'', "'\\''").replace("${{", "$ {{");
    format!("echo 'podrunner: the checkout with {why}' >&2\nexit 1")
}

/// Where the stand-in takes its tarball from.
enum Route<'a> {
    /// The local tree.
    Tree,
    /// A commit of the local git (T-640).
    Commit(&'a str),
    /// The tree of `OWNER/REPO` at its commit from the store (T-133).
    Other(&'a str, &'a str),
}

/// The stand-in of `actions/checkout` in the local mode (T-092): the
/// tarball of the local tree from the fake broker (`github.podrunner_broker` names
/// it) into the workspace, or into `path` in it. No `tar -C` and no
/// `tar -z`: in a podvm unit GNU `tar -C DIR` failed to open an existing
/// DIR (T-093).
/// With another repository (`OWNER/REPO` and its commit, T-133), the
/// tarball of that commit from the store, with its top directory
/// stripped. The local tree keeps the submodules to the depth `level`
/// (T-425).
fn checkout_script(path: &str, route: Route, level: u32) -> String {
    use podrunner_broker_fake::actions::{CHECKOUT, archive_name, checkout_at};
    let dest = if path.is_empty() { "$GITHUB_WORKSPACE".to_string() } else { format!("$GITHUB_WORKSPACE/{path}") };
    let (url, strip) = match route {
        Route::Tree => (CHECKOUT.to_string(), ""),
        Route::Commit(sha) => (checkout_at(sha), ""),
        Route::Other(repo, sha) => (format!("_actions/{}", archive_name(repo, sha)), " --strip-components=1"),
    };
    let script = CHECKOUT_SCRIPT.replace("DEST", &dest).replace("ROUTE", &url).replace("STRIP", strip);
    match route {
        Route::Tree => {
            script + &SUBMODULES.replace("LIST", crate::tree::SUBMODULES).replace("LEVEL", &level.to_string())
        }
        _ => script,
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

#[cfg(test)]
mod tests;
