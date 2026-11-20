//! `podrunner config init` (T-507, githome's G-37): a `podrunner.toml` for
//! `serve` from the unit kinds that this host can run, which `check`
//! takes. A program (githome's CLI) drives it: one line of JSON with
//! `--json`, a documented exit, and a second run with the same input
//! changes nothing. The file opens no TCP listener.

use std::path::{Path, PathBuf};

use pod_core::failure::Class;
use podrunner_slots::config::{Config, UnitConfig};
use serde_json::json;

use crate::cli::{Out, err, ok, opts};
use crate::text::display;

/// The kinds that `--kinds auto` tries, strongest boundary first.
const KINDS: [&str; 3] = ["microvm", "incus", "podvm"];

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
    slots: u32,
    kinds: Vec<String>,
}

/// The table of one unit kind.
fn unit(i: &Input, kind: &str) -> String {
    let mut t =
        format!("[unit.{kind}]\nslots = {}\nbinary = {}\nimages = {}\n", i.slots, quoted(&i.podvm), quoted(&i.images));
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
    let podvm = podrunner_podvm::Podvm { binary: i.podvm.clone().into() };
    let state = std::env::temp_dir().join(format!("podrunner-config-init-{}", std::process::id()));
    let mut unit = podrunner_podvm::PodvmUnit::new(podvm, &state, Path::new(&i.images)).map_err(|e| e.to_string())?;
    if let Some(p) = &u.podnet {
        unit = unit.with_podnet(Path::new(p)).map_err(|e| e.to_string())?;
    }
    let mut unit = unit.with_confine();
    let id = podrunner_schema::UnitId::parse("u-999999997").expect("a unit id");
    let proven = unit.verify_confine(&i.image, id);
    let _ = std::fs::remove_dir_all(&state);
    let why = match proven {
        Ok(true) => return Ok(()),
        Ok(false) => unit.confine_why().unwrap_or("no cause").to_string(),
        Err(e) => format!("its self-test: {e}"),
    };
    Err(format!("the confine layer that a trusted source needs is not proven here: {why}"))
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
    s.push_str(&format!(
        "\n[pin]\n[[pin.payload]]\narch = {}\nreference = {}\nbuilt_at_s = {}\n",
        quoted(std::env::consts::ARCH),
        quoted(&i.image),
        i.built_at_s
    ));
    if let Some(v) = &i.runner_version {
        s.push_str(&format!("runner_version = {}\n", quoted(v)));
    }
    s
}

fn input(rest: &[String]) -> Result<(Input, PathBuf, bool), Out> {
    let o = opts(rest)?;
    o.only(&[
        "--host",
        "--source",
        "--program",
        "--arg",
        "--key-file",
        "--tier",
        "--org",
        "--user",
        "--podvm",
        "--images",
        "--podnet",
        "--image",
        "--built-at-s",
        "--runner-version",
        "--slots",
        "--kinds",
        "--out",
    ])?;
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
        slots: u32::try_from(number("--slots", s("--slots").unwrap_or_else(|| "1".into()))?)
            .map_err(|_| err(Class::Usage, "--slots: too large"))?,
        kinds,
    };
    Ok((i, o.need("--out")?, o.flags.contains(&"--json".into())))
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
    crate::serve::plan(&cfg).map_err(|f| err(f.class, format!("check refuses the file: {}", f.detail)))?;
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
    write_private(&out, &body).map_err(|e| err(Class::Refused, format!("{}: {e}", out.display())))?;
    Ok(ok(say(true)))
}

/// Write `body` to a new file at mode 0600.
fn write_private(path: &Path, body: &str) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(path)?;
    f.write_all(body.as_bytes())
}
