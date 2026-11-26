//! The stand-in of `actions/checkout` as a local node action (T-697). The
//! runner prints a `uses:` line and its inputs for an action, but the whole
//! script for a `run:` step; so the stand-in is an action, `podrunner/checkout`,
//! that this broker serves from memory. Its inputs: `broker` (the base URL),
//! `route` (the tarball on it), `dest` (a path in the workspace), `strip`
//! (the top directories to drop), `list` and `level` (the submodules of the
//! local tree), and `fail` (a message that fails the step). The job token is
//! `ACTIONS_RUNTIME_TOKEN`, so it is in no input and in no log.
//!
//! The broker also resolves each `actions/checkout@REF` to this action, for
//! a step in an action that the job message cannot change (T-708). With no
//! `route`, it takes the inputs of `actions/checkout` and gives the local
//! tree, or asks the broker for the tree that the run pinned for another
//! repository or ref (T-713).

/// `OWNER/REPO` of the action.
pub const NAME: &str = "podrunner/checkout";

/// The commit that the broker resolves each ref of [`NAME`] to.
pub const SHA: &str = "0000000000000000000000000000000000000001";

/// The list of the submodules in the tarball of the local tree: one line
/// `DEPTH READY PATH` for each submodule, READY 1 when it has files. The
/// stand-in reads it, then removes it.
pub const LIST: &str = ".podrunner-submodules.list";

/// The levels of `submodules: recursive`.
pub const LEVELS: u32 = 10;

/// The inputs of `actions/checkout` v4 and v5 that the stand-in ignores:
/// it declares them, so the runner gives no warning for them.
const IGNORED: [&str; 16] = [
    "token",
    "ssh-key",
    "ssh-known-hosts",
    "ssh-strict",
    "ssh-user",
    "persist-credentials",
    "clean",
    "filter",
    "sparse-checkout",
    "sparse-checkout-cone-mode",
    "fetch-depth",
    "fetch-tags",
    "show-progress",
    "lfs",
    "set-safe-directory",
    "github-server-url",
];

fn action_yml() -> String {
    let mut y = String::from(
        "name: podrunner checkout\n\
description: The stand-in of actions/checkout in the local mode\n\
inputs:\n  broker:\n    description: The base URL of the broker\n    default: ${{ github.podrunner_broker }}\n  \
route:\n    description: The tarball on the broker\n  dest:\n    description: A path in the workspace\n  \
strip:\n    description: The top directories to drop\n  \
list:\n    description: The file of the submodules of the local tree\n  level:\n    description: The depth of the submodules\n  \
fail:\n    description: A message that fails the step\n  \
repository:\n    description: The repository of actions/checkout\n  ref:\n    description: The ref of actions/checkout\n  \
path:\n    description: The path of actions/checkout\n  submodules:\n    description: The submodules of actions/checkout\n",
    );
    for k in IGNORED {
        y.push_str(&format!("  {k}:\n    description: An input of actions/checkout that the stand-in ignores\n"));
    }
    y.push_str("runs:\n  using: node20\n  main: index.js\n");
    y
}

fn index_js() -> String {
    let ignored: Vec<String> = IGNORED.iter().map(|k| format!("'{k}'")).collect();
    INDEX_JS
        .replace("@CHECKOUT@", crate::actions::CHECKOUT)
        .replace("@REF_PIN@", crate::actions::REF_PIN)
        .replace("@PIN@", crate::actions::PIN)
        .replace("@LIST@", LIST)
        .replace("@LEVELS@", &LEVELS.to_string())
        .replace("@IGNORED@", &ignored.join(", "))
}

const INDEX_JS: &str = r#"'use strict';
const http = require('http'), https = require('https'), zlib = require('zlib');
const fs = require('fs'), path = require('path'), cp = require('child_process');
const input = (n) => process.env['INPUT_' + n.toUpperCase()] || '';
const die = (m, c) => { console.error('podrunner: ' + m); process.exit(c || 1); };
const ws = path.resolve(process.env.GITHUB_WORKSPACE || '.');
let a, dir;
if (input('fail')) die('the checkout with ' + input('fail'));
if (input('route')) start({ route: input('route'), dest: input('dest'), strip: input('strip'), list: input('list'), level: input('level') });
else checkout();
// A GET of the broker with the job token: `done` takes the status and the body.
function get(route, what, done) {
  const u = input('broker') + route;
  const h = { Authorization: 'Bearer ' + (process.env.ACTIONS_RUNTIME_TOKEN || '') };
  (u.startsWith('https') ? https : http).get(u, { headers: h }, (r) => {
    const b = [];
    r.on('data', (c) => b.push(c));
    r.on('end', () => done(r.statusCode, Buffer.concat(b)));
  }).on('error', (e) => die(what + ': ' + e.message, 7));
}
function start(x) {
  a = x;
  dir = path.resolve(ws, a.dest);
  if (path.relative(ws, dir).split(path.sep)[0] === '..') die('the path ' + a.dest + ' is not in the workspace');
  fs.mkdirSync(dir, { recursive: true });
  get(a.route, 'the tree', (s, b) => (s === 200 ? unpack(b) : die('the tree: HTTP ' + s, 22)));
}
// `uses: actions/checkout` in an action (T-708): the local tree, into
// `path`, with its `submodules`. Another repository or another ref takes
// the tree that the run pinned for the static values of a local action
// (T-713); with no pin (an expression), the step fails.
function checkout() {
  const e = process.env, repo = input('repository').trim(), ref = input('ref').trim(), sub = input('submodules');
  const level = { '': 0, 'false': 0, 'true': 1, 'recursive': @LEVELS@ }[sub];
  if (level === undefined) die('the submodules of the checkout must be true, false or recursive, not ' + sub);
  const ignored = [@IGNORED@].filter((k) => input(k));
  if (ignored.length) console.log('podrunner: the stand-in of actions/checkout ignores ' + ignored.join(', '));
  const tree = { route: '@CHECKOUT@', dest: input('path'), strip: '', list: '@LIST@', level: String(level) };
  const same = !repo || repo.toLowerCase() === (e.GITHUB_REPOSITORY || '').toLowerCase();
  if (same && (!ref || [e.GITHUB_SHA, e.GITHUB_REF, e.GITHUB_REF_NAME].includes(ref))) return start(tree);
  const what = same ? 'the ref ' + ref : repo + (ref ? ' at ' + ref : '');
  const key = same ? '@REF_PIN@' + ref : repo + '@' + (ref || 'HEAD');
  get('@PIN@' + encodeURIComponent(key), 'the pin of ' + what, (s, b) => {
    if (s === 404) {
      die('the checkout of ' + what + ' in an action: the run pinned no tree for it; fix: give `repository` and `ref` as text in a local action, or check it out in a step of the workflow');
    }
    if (s !== 200) die('the pin of ' + what + ': HTTP ' + s, 22);
    const p = JSON.parse(b.toString());
    if (p.fail) die('the checkout of ' + what + ': ' + p.fail);
    if (p.route === tree.route) return start(tree);
    if (level > 0) die('the checkout of ' + what + ' with submodules: the stand-in gives submodules only in the local tree');
    start({ route: p.route, dest: tree.dest, strip: p.strip || '', list: '', level: '0' });
  });
}
function unpack(gz) {
  const t = ['-xf', '-'];
  if (a.strip) t.push('--strip-components=' + a.strip);
  const p = cp.spawnSync('tar', t, { cwd: dir, input: zlib.gunzipSync(gz), stdio: ['pipe', 'inherit', 'inherit'] });
  if (p.error) die('the stand-in of actions/checkout needs tar in the job container');
  if (p.status !== 0) die('tar failed', p.status || 1);
  submodules();
}
// Each submodule one level past `level` is an empty directory, as GitHub
// leaves it; one to `level` with no files fails the step.
function submodules() {
  const m = path.join(dir, a.list);
  if (!a.list || !fs.existsSync(m)) return;
  const level = Number(a.level);
  const rows = fs.readFileSync(m, 'utf8').split('\n').map((l) => /^(\d+) (\d+) (.*)$/.exec(l)).filter(Boolean);
  fs.rmSync(m, { force: true });
  for (const [, d, r, p] of rows) {
    if (Number(d) <= level) {
      if (r === '0') die('the submodule ' + p + ' has no files in the local tree; fix: git submodule update --init --recursive');
    } else if (Number(d) === level + 1) {
      fs.rmSync(path.join(dir, p), { recursive: true, force: true });
      fs.mkdirSync(path.join(dir, p), { recursive: true });
    }
  }
}
"#;

/// The tarball of the action: one top directory, as a GitHub tarball.
pub fn tarball() -> Vec<u8> {
    let mut tar = Vec::new();
    for (name, text) in [("action.yml", action_yml()), ("index.js", index_js())] {
        tar_entry(&mut tar, &format!("podrunner-checkout/{name}"), text.as_bytes());
    }
    tar.extend_from_slice(&[0; 1024]);
    gzip(&tar)
}

fn tar_entry(out: &mut Vec<u8>, name: &str, body: &[u8]) {
    let mut h = [0u8; 512];
    let mut put = |at: usize, text: &str| h[at..at + text.len()].copy_from_slice(text.as_bytes());
    put(0, name);
    put(100, "0000644\0");
    put(108, "0000000\0");
    put(116, "0000000\0");
    put(124, &format!("{:011o}\0", body.len()));
    put(136, "00000000000\0");
    put(148, "        ");
    put(156, "0");
    put(257, "ustar\0");
    put(263, "00");
    let sum: u32 = h.iter().map(|b| u32::from(*b)).sum();
    h[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
    out.extend_from_slice(&h);
    out.extend_from_slice(body);
    out.resize(out.len().div_ceil(512) * 512, 0);
}

/// A gzip file of stored blocks: the bytes of the action are few.
fn gzip(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 0xff];
    let mut blocks = data.chunks(65535).peekable();
    while let Some(b) = blocks.next() {
        out.push(u8::from(blocks.peek().is_none()));
        out.extend_from_slice(&(b.len() as u16).to_le_bytes());
        out.extend_from_slice(&(!(b.len() as u16)).to_le_bytes());
        out.extend_from_slice(b);
    }
    out.extend_from_slice(&crc32(data).to_le_bytes());
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out
}

fn crc32(data: &[u8]) -> u32 {
    let mut c = !0u32;
    for b in data {
        c ^= u32::from(*b);
        for _ in 0..8 {
            c = (c >> 1) ^ (0xedb8_8320 & (c & 1).wrapping_neg());
        }
    }
    !c
}

#[cfg(test)]
mod tests;
