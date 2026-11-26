//! The stand-in of `actions/checkout` as a local node action (T-697). The
//! runner prints a `uses:` line and its inputs for an action, but the whole
//! script for a `run:` step; so the stand-in is an action, `podrunner/checkout`,
//! that this broker serves from memory. Its inputs: `broker` (the base URL),
//! `route` (the tarball on it), `dest` (a path in the workspace), `strip`
//! (the top directories to drop), `list` and `level` (the submodules of the
//! local tree), and `fail` (a message that fails the step). The job token is
//! `ACTIONS_RUNTIME_TOKEN`, so it is in no input and in no log.

/// `OWNER/REPO` of the action.
pub const NAME: &str = "podrunner/checkout";

/// The commit that the broker resolves each ref of [`NAME`] to.
pub const SHA: &str = "0000000000000000000000000000000000000001";

const ACTION_YML: &str = "name: podrunner checkout\n\
description: The stand-in of actions/checkout in the local mode\n\
inputs:\n  broker:\n    description: The base URL of the broker\n  route:\n    description: The tarball on the broker\n  \
dest:\n    description: A path in the workspace\n  strip:\n    description: The top directories to drop\n  \
list:\n    description: The file of the submodules of the local tree\n  level:\n    description: The depth of the submodules\n  \
fail:\n    description: A message that fails the step\n\
runs:\n  using: node20\n  main: index.js\n";

const INDEX_JS: &str = r#"'use strict';
const http = require('http'), https = require('https'), zlib = require('zlib');
const fs = require('fs'), path = require('path'), cp = require('child_process');
const input = (n) => process.env['INPUT_' + n.toUpperCase()] || '';
const die = (m, c) => { console.error('podrunner: ' + m); process.exit(c || 1); };
if (input('fail')) die('the checkout with ' + input('fail'));
const dir = path.join(process.env.GITHUB_WORKSPACE || '.', input('dest'));
fs.mkdirSync(dir, { recursive: true });
const u = input('broker') + input('route');
const h = { Authorization: 'Bearer ' + (process.env.ACTIONS_RUNTIME_TOKEN || '') };
(u.startsWith('https') ? https : http).get(u, { headers: h }, (r) => {
  if (r.statusCode !== 200) { r.resume(); die('the tree: HTTP ' + r.statusCode, 22); }
  const b = [];
  r.on('data', (c) => b.push(c));
  r.on('end', () => unpack(Buffer.concat(b)));
}).on('error', (e) => die('the tree: ' + e.message, 7));
function unpack(gz) {
  const a = ['-xf', '-'];
  if (input('strip')) a.push('--strip-components=' + input('strip'));
  const p = cp.spawnSync('tar', a, { cwd: dir, input: zlib.gunzipSync(gz), stdio: ['pipe', 'inherit', 'inherit'] });
  if (p.error) die('the stand-in of actions/checkout needs tar in the job container');
  if (p.status !== 0) die('tar failed', p.status || 1);
  submodules();
}
// Each submodule one level past `level` is an empty directory, as GitHub
// leaves it; one to `level` with no files fails the step.
function submodules() {
  const m = path.join(dir, input('list'));
  if (!input('list') || !fs.existsSync(m)) return;
  const level = Number(input('level'));
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
    for (name, text) in [("action.yml", ACTION_YML), ("index.js", INDEX_JS)] {
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
mod tests {
    #[test]
    fn the_tarball_is_a_gzip_of_a_tar_with_the_action() {
        assert_eq!(super::crc32(b"123456789"), 0xcbf4_3926);
        let gz = super::tarball();
        let dir = std::env::temp_dir().join(format!("podrunner-standin-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.tgz"), &gz).unwrap();
        let st = std::process::Command::new("tar")
            .args(["-xzf", "a.tgz", "--strip-components=1"])
            .current_dir(&dir)
            .status()
            .unwrap();
        assert!(st.success(), "tar reads the tarball");
        assert_eq!(std::fs::read_to_string(dir.join("action.yml")).unwrap(), super::ACTION_YML);
        assert_eq!(std::fs::read_to_string(dir.join("index.js")).unwrap(), super::INDEX_JS);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
