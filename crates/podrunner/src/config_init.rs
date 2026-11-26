//! `podrunner config init` (T-507, githome's G-37): a `podrunner.toml` for
//! `serve` from the unit kinds that this host can run, which `check`
//! takes. A program (githome's CLI) drives it: one line of JSON with
//! `--json`, a documented exit, and a second run with the same input
//! changes nothing. The file opens no TCP listener.

use std::path::{Path, PathBuf};

use pod_core::failure::Class;
use podrunner_slots::config::{Config, Pins, UnitConfig};
use serde_json::json;

use crate::cli::{Out, err, ok, opts};
use crate::text::display;

/// The kinds that `--kinds auto` tries, strongest boundary first.
const KINDS: [&str; 3] = ["microvm", "incus", "podvm"];
/// The runner directory of the runner image of a release (`images/runner/`).
const RUNNER_DIR: &str = "/home/runner";

/// One value of TOML: a string with its quotes and escapes. The escapes of
/// a JSON string are escapes of a basic string of TOML.
fn quoted(s: &str) -> String {
    serde_json::to_string(s).expect("a string is JSON")
}

fn list(items: &[String]) -> String {
    format!("[{}]", items.iter().map(|s| quoted(s)).collect::<Vec<_>>().join(", "))
}

/// The input of `config init`.
struct Input {
    host: String,
    source: String,
    program: String,
    args: Vec<String>,
    key_file: Option<String>,
    tier: String,
    orgs: Vec<String>,
    users: Vec<String>,
    podvm: String,
    images: String,
    podnet: Option<String>,
    image: String,
    built_at_s: u64,
    runner_version: Option<String>,
    /// The bytes of the image unpacked (`podrunner-manifest.json`, T-572).
    unpacked_bytes: Option<u64>,
    slots: u32,
    kinds: Vec<String>,
}

/// The table of one unit kind.
fn unit(i: &Input, kind: &str) -> String {
    let mut t =
        format!("[unit.{kind}]\nslots = {}\nbinary = {}\nimages = {}\n", i.slots, quoted(&i.podvm), quoted(&i.images));
    // The runner of the image by its absolute path: a podvm unit runs in a
    // work directory of its own, a guest in its own (T-565); the default
    // `./run.sh` is not there.
    t.push_str(&format!("command = [{}]\n", quoted(&format!("{RUNNER_DIR}/run.sh"))));
    if kind == "podvm" {
        t.push_str(&format!("runner_dir = {}\n", quoted(RUNNER_DIR)));
    }
    // The hosted work folder of the runner image (T-590): GITHUB_WORKSPACE
    // is /home/runner/work/REPO/REPO, as on a hosted runner.
    t.push_str(&format!("work_folder = {}\n", quoted(&format!("{RUNNER_DIR}/work"))));
    if kind == "podvm"
        && let Some(p) = &i.podnet
    {
        t.push_str(&format!("podnet = {}\n", quoted(p)));
    }
    // The trusted floor is `confined`: a podvm kind reaches it only with
    // the confine layer, which `serve` proves at its start (T-565).
    if kind == "podvm" && i.tier == "trusted" {
        t.push_str("confine = true\n");
    }
    t
}

/// The kinds of `i` that this host can run: a probe of each, as `doctor`
/// does.
fn probed(i: &Input) -> (Vec<String>, Vec<(String, String)>) {
    let (mut ok, mut no) = (Vec::new(), Vec::new());
    for kind in &i.kinds {
        let u: Result<UnitConfig, String> = toml::from_str(&unit(i, kind).replacen(&format!("[unit.{kind}]\n"), "", 1))
            .map_err(|e| e.message().to_string());
        let proven = |u: UnitConfig| {
            crate::doctor::capability(kind, &u)?;
            // A kind whose bound the image exceeds refuses each job (T-573).
            let pins: Pins = toml::from_str(&format!("[[payload]]\n{}", payload(i))).map_err(|e| e.to_string())?;
            crate::pin_size::kind_fits(kind, &u, &pins).map_err(|f| f.detail)?;
            // With no hold a trusted podvm kind serves nothing anyway, and
            // `check` says so (`no-tier-served`): no self-test then.
            match kind == "podvm" && u.confine && u.podnet.is_some() {
                true => confine_proven(i, &u),
                false => Ok(()),
            }
        };
        match u.and_then(proven) {
            Ok(()) => ok.push(kind.clone()),
            Err(why) => no.push((kind.clone(), why)),
        }
    }
    (ok, no)
}

/// The self-test of the confine layer that `serve` runs at its start, with
/// the hold and the pinned image of the file (T-565): a trusted podvm kind
/// whose layer is not proven here would serve nothing, so it is refused
/// with the cause.
fn confine_proven(i: &Input, u: &UnitConfig) -> Result<(), String> {
    crate::podvm_kind::confine_proven(&i.podvm, &i.images, u.podnet.as_deref(), &i.image)
}

/// The text of the file with the kinds `kinds`.
fn text(i: &Input, kinds: &[String]) -> String {
    let mut s = String::from("# podrunner.toml of `podrunner config init` (T-507).\n");
    s.push_str(&format!(
        "host = {}\n\n[[source]]\nname = {}\nkind = \"program\"\n",
        quoted(&i.host),
        quoted(&i.source)
    ));
    s.push_str(&format!(
        "tier = {}\narch = {}\nprogram = {}\n",
        quoted(&i.tier),
        quoted(std::env::consts::ARCH),
        quoted(&i.program)
    ));
    if !i.args.is_empty() {
        s.push_str(&format!("program_args = {}\n", list(&i.args)));
    }
    if let Some(k) = &i.key_file {
        s.push_str(&format!("key_file = {}\n", quoted(k)));
    }
    for k in kinds {
        s.push('\n');
        s.push_str(&unit(i, k));
    }
    let logins = |v: &[String]| v.iter().map(|l| format!("{{ login = {} }}", quoted(l))).collect::<Vec<_>>().join(", ");
    s.push_str(&format!("\n[trust]\norganizations = [{}]\nusers = [{}]\n", logins(&i.orgs), logins(&i.users)));
    s.push_str(&format!("\n[pin]\n[[pin.payload]]\n{}", payload(i)));
    s
}

/// The keys of the pinned payload.
fn payload(i: &Input) -> String {
    let mut s = format!(
        "arch = {}\nreference = {}\nbuilt_at_s = {}\n",
        quoted(std::env::consts::ARCH),
        quoted(&i.image),
        i.built_at_s
    );
    if let Some(v) = &i.runner_version {
        s.push_str(&format!("runner_version = {}\n", quoted(v)));
    }
    if let Some(n) = i.unpacked_bytes {
        s.push_str(&format!("unpacked_bytes = {n}\n"));
    }
    s
}

fn input(rest: &[String]) -> Result<(Input, PathBuf, bool), Out> {
    let o = opts("config init", rest)?;
    let s = |k: &str| o.get(k).map(|p| p.display().to_string());
    let need = |k: &str| o.need(k).map(|p| p.display().to_string());
    let all = |k: &str| o.values.iter().filter(|(n, _)| n == k).map(|(_, v)| v.clone()).collect::<Vec<_>>();
    let number =
        |k: &str, v: String| v.parse::<u64>().map_err(|_| err(Class::Usage, format!("{k} {v:?}: not a number")));
    let tier = s("--tier").unwrap_or_else(|| "trusted".into());
    if !["trusted", "untrusted"].contains(&tier.as_str()) {
        return Err(err(Class::Usage, format!("--tier {:?}: trusted or untrusted", display(&tier))));
    }
    let kinds = match s("--kinds").as_deref() {
        None | Some("auto") => KINDS.iter().map(|k| k.to_string()).collect(),
        Some(l) => l.split(',').map(str::to_string).collect::<Vec<_>>(),
    };
    if let Some(k) = kinds.iter().find(|k| !KINDS.contains(&k.as_str())) {
        return Err(err(Class::Usage, format!("--kinds: {:?} is not microvm, incus or podvm", display(k))));
    }
    let host = match s("--host") {
        Some(h) => h,
        None => std::fs::read_to_string("/proc/sys/kernel/hostname").map(|h| h.trim().to_string()).unwrap_or_default(),
    };
    let i = Input {
        host,
        source: need("--source")?,
        program: need("--program")?,
        args: all("--arg"),
        key_file: s("--key-file"),
        tier,
        orgs: all("--org"),
        users: all("--user"),
        podvm: need("--podvm")?,
        images: need("--images")?,
        podnet: s("--podnet"),
        image: need("--image")?,
        built_at_s: number("--built-at-s", need("--built-at-s")?)?,
        runner_version: s("--runner-version"),
        unpacked_bytes: s("--unpacked-bytes").map(|v| number("--unpacked-bytes", v)).transpose()?,
        slots: u32::try_from(number("--slots", s("--slots").unwrap_or_else(|| "1".into()))?)
            .map_err(|_| err(Class::Usage, "--slots: too large"))?,
        kinds,
    };
    values(&i)?;
    Ok((i, o.need("--out")?, o.flags.contains(&"--json".into())))
}

/// The values that give a file that no job can use (T-635): no slot, an
/// empty host, and a login that is empty or has a space, a control
/// character or a `/` (a login of a forge has none of them).
fn values(i: &Input) -> Result<(), Out> {
    if i.slots == 0 {
        return Err(err(Class::Usage, "--slots 0: no unit could run; give 1 or more"));
    }
    let bad = |v: &str| v.trim().is_empty() || v.chars().any(|c| c.is_whitespace() || c.is_control());
    if bad(&i.host) {
        return Err(err(Class::Usage, format!("--host {:?}: give a name with no space", display(&i.host))));
    }
    for (flag, logins) in [("--org", &i.orgs), ("--user", &i.users)] {
        if let Some(l) = logins.iter().find(|l| bad(l) || l.contains('/')) {
            return Err(err(Class::Usage, format!("{flag} {:?}: give a login with no space and no '/'", display(l))));
        }
    }
    Ok(())
}

/// `podrunner config init ...`: write the file, or say that it is there
/// with the same text. Exit 0; 64 for a bad flag; 78 when no kind runs
/// here or `check` refuses the file; 77 when OUT holds another text.
pub(crate) fn init(rest: &[String]) -> Result<Out, Out> {
    let (i, out, json) = input(rest)?;
    let (kinds, refused) = probed(&i);
    let say = |changed: bool| {
        if json {
            let refused: Vec<_> = refused.iter().map(|(k, w)| json!({ "kind": k, "why": w })).collect();
            format!(
                "{}\n",
                json!({ "file": out.display().to_string(), "kinds": kinds, "refused": refused, "changed": changed })
            )
        } else {
            let verb = if changed { "written" } else { "unchanged" };
            let mut t = format!("{}: {verb}: kinds {}\n", display(&out.display().to_string()), kinds.join(", "));
            for (k, w) in &refused {
                t.push_str(&format!("kind {k}: not here: {}\n", display(w)));
            }
            t
        }
    };
    if kinds.is_empty() {
        let why: Vec<String> = refused.iter().map(|(k, w)| format!("{k}: {w}")).collect();
        return Err(err(Class::Config, format!("no unit kind runs on this host ({})", display(&why.join("; ")))));
    }
    let body = text(&i, &kinds);
    let cfg = Config::parse(&body).map_err(|e| err(Class::Config, format!("the file would not parse: {e}")))?;
    // The kinds that the probe refused explain a refusal of check (T-629).
    let not_here: String = refused.iter().map(|(k, w)| format!("; kind {k}: not here: {w}")).collect();
    crate::serve::plan(&cfg, true)
        .map_err(|f| err(f.class, format!("check refuses the file: {}{}", f.detail, display(&not_here))))?;
    match std::fs::read_to_string(&out) {
        Ok(old) if old == body => return Ok(ok(say(false))),
        Ok(_) => {
            return Err(err(
                Class::Refused,
                format!("{}: holds another configuration: remove it, or give another --out", out.display()),
            ));
        }
        Err(_) => {}
    }
    own_parent(&out).map_err(|e| err(Class::Refused, format!("{}: {e}", out.display())))?;
    write_private(&out, &body).map_err(|e| err(Class::Refused, format!("{}: {e}", out.display())))?;
    Ok(ok(say(true)))
}

/// Make the missing levels of the parent of `path` at mode 0700, so each
/// level that this call makes is a directory of this user. A parent that
/// is there stays as it is (`/tmp` is not ours): the write refuses a
/// parent that is no directory or not writable.
fn own_parent(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    match path.parent() {
        Some(p) if !p.as_os_str().is_empty() && !p.exists() => {
            std::fs::DirBuilder::new().recursive(true).mode(0o700).create(p)
        }
        _ => Ok(()),
    }
}

/// Write `body` to a new file at mode 0600.
fn write_private(path: &Path, body: &str) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(path)?;
    f.write_all(body.as_bytes())
}
