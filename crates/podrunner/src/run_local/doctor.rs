//! `podrunner doctor --run FILE [the options of run]` (T-153): the checks of
//! the inputs of a local run before it starts. Each check that fails names
//! its fix; nothing changes.

use std::path::Path;

use podrunner_podvm::Podvm;
use podrunner_podvm::probe::{
    LOWEST, LOWEST_ENGINE, LOWEST_ENGINE_INCUS, LOWEST_GUEST, LOWEST_IMAGE_LABELS, LOWEST_INCUS, LOWEST_INCUS_INIT,
    LOWEST_MACHINE_INIT,
};

use super::Args;
use env::{ca_files, is_proxy, proxy_line};

mod env;

/// One check: its name, whether it holds, and the detail or the fix.
pub(super) struct Check {
    pub name: &'static str,
    pub ok: bool,
    pub text: String,
}

fn check(name: &'static str, r: Result<String, String>) -> Check {
    match r {
        Ok(text) => Check { name, ok: true, text },
        Err(text) => Check { name, ok: false, text },
    }
}

/// The scratch disk of a run: a podvm unit unpacks its image under TMPDIR,
/// about eight times the size of the image in the store (2.4 GiB for the
/// base image), for each job at a time (T-325); at least 2 GiB.
fn need(a: &Args) -> u64 {
    let size = || super::dry::image_size(a, a.image.as_ref().filter(|_| a.podvm.is_some())?);
    (size().map_or(0, |s| s * 8)).max(2 << 30) * a.parallel.max(1) as u64
}

/// The free bytes of the file system of `dir`, against `need` bytes.
fn free_disk(dir: &Path, need: u64) -> Result<String, String> {
    let c = std::ffi::CString::new(dir.as_os_str().as_encoded_bytes()).map_err(|e| e.to_string())?;
    // SAFETY: statvfs writes into a zeroed struct that this function owns.
    let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c.as_ptr(), &mut s) } != 0 {
        return Err(format!(
            "{}: {}; fix: set TMPDIR to a directory that exists and that you can write",
            dir.display(),
            std::io::Error::last_os_error()
        ));
    }
    let free = s.f_bavail as u64 * s.f_frsize as u64;
    // Below 1 GiB, in MiB: "0 GiB" reads as no space at all (T-218).
    let size = if free >> 30 == 0 { format!("{} MiB", free >> 20) } else { format!("{} GiB", free >> 30) };
    let want = format!("{:.1} GiB", need as f64 / f64::from(1u32 << 30));
    if free < need {
        return Err(format!(
            "{}: {size} free; this run needs about {want} (each job at a time unpacks its image); fix: free the disk, set TMPDIR, or give a lower --jobs",
            dir.display()
        ));
    }
    Ok(format!("{}: {size} free; this run needs about {want}", dir.display()))
}

/// `podvm inspect IMAGE` in `store`: the image is there.
pub(super) fn image_in_store(podvm: &Path, store: &Path, image: &str) -> Result<String, String> {
    let out = std::process::Command::new(podvm)
        .args(["inspect", image])
        .env("PODVM_STORE", store)
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| format!("{}: {e}", podvm.display()))?;
    match out.status.success() {
        true => Ok(format!("{image} is in {}", store.display())),
        false => Err(format!(
            "{} does not hold {image}; fix: load the runner image (PODVM_STORE={} podvm load -i runner-ARCH.tar) or name another --image",
            store.display(),
            store.display()
        )),
    }
}

/// The tar of `image` in the image cache of the user, when there is one,
/// is whole: a cut or changed tar fails each run that loads it (T-630).
pub(super) fn cached_tar(dir: &Path, image: &str) -> Result<String, String> {
    // The cache keeps a tar by its digest: with no digest, "no tar yet"
    // would be false.
    let Some(digest) = image.rsplit_once('@').map(|(_, d)| d).filter(|d| d.starts_with("sha256:")) else {
        return Err(format!(
            "{image} names no digest; the cache keeps a tar by its digest; fix: give --image NAME@sha256:HEX"
        ));
    };
    let tar = dir.join(podrunner_podvm::tar_name(digest));
    if !tar.is_file() {
        return Ok(format!("{}: no tar of {image} yet; the first run saves it", dir.display()));
    }
    match podrunner_podvm::tar_fault(&tar) {
        None => Ok(format!("{} is whole", tar.display())),
        Some(fault) => Err(format!("{} is not whole ({fault}); fix: rm {}", tar.display(), tar.display())),
    }
}

/// The egress hold of the units of the run (T-226).
pub(super) fn hold(a: &Args) -> String {
    match (&a.podvm, &a.guest, &a.podnet, &a.runner_dir) {
        (Some(_), _, Some(_), _) => "podnet, the seccomp hold (advisory): only the hosts of --allow".into(),
        (Some(_), _, None, _) => "none: the unit has the network of the host (give --podnet for a hold)".into(),
        (None, Some(_), _, _) => "the guest has no NIC: only the broker and the hosts of --allow (enforced)".into(),
        _ => "none: the runner directory has the network of the host".into(),
    }
}

/// The runner reads each directory above its root, up to `/` (T-213).
fn parents_readable(dir: &Path) -> Result<String, String> {
    let dir = std::path::absolute(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    // A path that is not there is the check of runner-dir (T-289).
    if !dir.exists() {
        return Ok(format!("{} is not there: see runner-dir", dir.display()));
    }
    // From the root down: the first directory that cannot be read is the
    // cause, not a directory under it (T-277).
    let parents: Vec<&Path> = dir.ancestors().skip(1).collect();
    for p in parents.into_iter().rev() {
        std::fs::read_dir(p).map_err(|e| {
            format!(
                "{}: {e}; fix: the runner reads each directory above its root: give it a read of {}, or put the runner directory where each parent can be read",
                p.display(),
                p.display()
            )
        })?;
    }
    Ok(format!("each directory above {} can be read", dir.display()))
}

/// The checks of a run with the options `a`.
pub(super) fn checks(a: &Args) -> Vec<Check> {
    let mut out = vec![
        check("file-size-limit", super::dry::file_size_limit(a)),
        check("scratch-disk", free_disk(&std::env::temp_dir(), need(a))),
    ];
    out.push(check(
        "workflow",
        std::fs::read_to_string(&a.file)
            .map_err(|e| format!("{}: {e}", a.file.display()))
            .and_then(|t| pod_actions_workflow::parse(&t).map(|_| "it parses".to_string()).map_err(|e| e.to_string())),
    ));
    // Each job needs a fake broker on the loopback (T-213), but under the
    // vnet and in a guest, where it listens on a unix socket (T-220).
    out.push(check(
        "loopback",
        match a.vnet.is_some() || a.guest.is_some() {
            true => Ok("not needed: the broker of each job listens on a unix socket".to_string()),
            false => std::net::TcpListener::bind("127.0.0.1:0")
                .map(|_| "a bind of 127.0.0.1:0 holds".to_string())
                .map_err(|e| format!("bind 127.0.0.1:0: {e}; {}", podrunner_local::exec::BIND_FIX)),
        },
    ));
    // The host paths that a unit kind mounts (T-476).
    if a.podvm.is_some() || a.guest.is_some() {
        out.push(check("host-paths", super::dry::mounted(a).map(|()| "no ':' in a mounted path".to_string())));
    }
    // Each path of a flag (T-321): a file to read, or a directory to use.
    let files = [
        ("--secret-file", &a.secret_file),
        ("--env-file", &a.env_file),
        ("--var-file", &a.var_file),
        ("--input-file", &a.input_file),
    ];
    for (flag, f) in files {
        if let Some(f) = f {
            out.push(check(
                flag,
                std::fs::File::open(f)
                    .map(|_| f.display().to_string())
                    .map_err(|e| format!("{}: {e}; fix: give a file that you can read", f.display())),
            ));
        }
    }
    for (flag, d) in [("--checkout", &a.checkout), ("--toolcache", &a.toolcache)] {
        if let Some(d) = d {
            out.push(check(
                flag,
                match d.is_dir() {
                    true => Ok(d.display().to_string()),
                    false => Err(format!("{}: not a directory; fix: give a directory that exists", d.display())),
                },
            ));
        }
    }
    let dirs = [("--log-dir", &a.log_dir), ("--export", &a.export), ("--state", &a.state), ("--actions", &a.actions)];
    for (flag, d) in dirs {
        if let Some(d) = d {
            out.push(check(flag, writable(d)));
        }
    }
    let lock = a.file.parent().unwrap_or(Path::new(".")).join(podrunner_local::lock::LOCK_FILE);
    out.push(check(
        "lock",
        if !a.file.is_file() {
            // The lock is beside the workflow file (T-635).
            Ok("not checked: the workflow file is not there".into())
        } else if lock.exists() {
            podrunner_local::lock::Lock::read(&lock)
                .map(|l| format!("{}: {} references", lock.display(), l.pins.len() + l.images.len()))
                .map_err(|e| e.to_string())
        } else {
            Ok(format!("{}: none yet; a run with no --frozen writes it", lock.display()))
        },
    ));
    // The proxy and the CA files of the user, as a job takes them: on a
    // runner directory and in a podvm unit (T-635). A guest has no NIC. A
    // held unit takes the CA files only: its hold is its way out.
    if a.guest.is_none() && (a.runner_dir.is_some() || a.podvm.is_some()) {
        let unit = a.podvm.is_some();
        let held = unit
            && (a.podnet.is_some() || (a.vnet.is_some() && !(a.allow.is_empty() && a.environment_allow.is_empty())));
        let vars: Vec<(String, String)> = std::env::vars().filter(|(k, _)| !held || !is_proxy(k)).collect();
        out.push(check("proxy", Ok(proxy_line(&vars, !unit))));
        out.push(check("ca-files", ca_files(&vars, unit)));
    }
    if let Some(dir) = &a.runner_dir {
        out.push(check("runner-dir-parents", parents_readable(dir)));
        let ok = dir.join("run.sh").is_file() && dir.join("bin/Runner.Listener").is_file();
        out.push(check(
            "runner-dir",
            if let (true, Some(e)) = (ok, podrunner_local::exec::space_refusal(dir)) {
                Err(e)
            } else if ok {
                Ok(dir.display().to_string())
            } else {
                Err(format!("{}: no unpacked runner; fix: unpack a release of actions/runner there", dir.display()))
            },
        ));
    }
    let guest_engine = super::guest::has_engine(a);
    let (binary, lowest, flag) = match (&a.podvm, &a.guest) {
        (Some(b), _) => (Some(b.clone()), LOWEST, "--podvm"),
        (None, Some((true, b))) => {
            // The order of `microvm_kind::plan` and `incus_kind::plan`.
            let lowest = if a.init {
                LOWEST_MACHINE_INIT
            } else if a.engine.is_some() {
                LOWEST_ENGINE
            } else if guest_engine {
                LOWEST_IMAGE_LABELS
            } else {
                LOWEST_GUEST
            };
            (Some(b.clone()), lowest, "--microvm")
        }
        (None, Some((false, b))) => {
            let lowest = match (a.init, guest_engine) {
                (true, _) => LOWEST_INCUS_INIT,
                (false, true) => LOWEST_ENGINE_INCUS,
                (false, false) => LOWEST_INCUS,
            };
            (Some(b.clone()), lowest, "--incus")
        }
        _ => (None, LOWEST, ""),
    };
    if let Some(b) = binary {
        let podvm = Podvm { binary: b.clone() };
        // Only a podvm that answers with a low version needs another
        // version; a wrong path needs another path (T-212).
        let fix = |e: podrunner_unit::UnitError| match (b.is_file(), e.code) {
            (false, _) => format!("{flag} {}: not a file; fix: give the path of a podvm binary", b.display()),
            (true, podrunner_unit::UnitCode::Unsupported) => format!("{e}; fix: take podvm {lowest} or later"),
            (true, _) => format!(
                "{flag} {}: {e}; fix: give the path of a podvm binary (`podvm probe --json` must answer)",
                b.display()
            ),
        };
        out.push(check(
            "podvm",
            podvm.require(lowest).map(|v| format!("{} is {v} ({lowest} or later)", b.display())).map_err(fix),
        ));
        // The rung of podvm on this host (T-300): only the namespace rung
        // keeps a static binary of a job from the files of the user.
        if a.guest.is_none()
            && let Ok(info) = std::process::Command::new(&b).args(["system", "info"]).output()
            && let Some(r) = rung(&String::from_utf8_lossy(&info.stdout))
        {
            out.push(check("rung", Ok(rung_text(&r, &String::from_utf8_lossy(&info.stdout)))));
        }
        // A job with containers needs the hooks of podvm (T-311).
        if a.guest.is_none()
            && let Ok(c) = podvm.probe()
        {
            out.push(check("containers", Ok(match c.container == podrunner_schema::ContainerRoute::Hooks {
                true => "container: and docker:// steps run through the hooks of the unit".into(),
                false => "podvm gives no container hooks here: a job with container:, services: or docker:// steps needs --microvm and --engine".into(),
            })));
        }
        if let Some(store) = &a.images {
            out.push(check(
                "store",
                if store.is_dir() {
                    Ok(store.display().to_string())
                } else {
                    Err(format!("{}: not a directory; fix: give the podvm store with --images", store.display()))
                },
            ));
            // The store holds the image of `--image` (T-223).
            if let (Some(image), true) = (&a.image, store.is_dir()) {
                out.push(check("image", image_in_store(&b, store, image)));
            }
            if let (Some(image), Ok(dir)) = (&a.image, super::actions::user_cache("image-tars")) {
                out.push(check("image-cache", cached_tar(&dir, image)));
            }
        }
    }
    if matches!(a.guest, Some((true, _))) {
        out.push(check("kvm", super::guest::kvm().map(|()| "/dev/kvm opens".into())));
        out.push(check("mem", super::guest::mem_line(a)));
        if let Some(e) = &a.engine {
            out.push(check(
                "engine",
                if e.join("dockerd").is_file() {
                    Ok(e.display().to_string())
                } else {
                    Err(format!("{}: no dockerd; fix: unpack the static Docker binaries there", e.display()))
                },
            ));
        }
    }
    if let Some(p) = &a.podnet {
        out.push(check("podnet", podnet(a, p)));
    }
    out.push(check("hold", Ok(hold(a))));
    out
}

/// `--podnet`: a file, and the seccomp hold that `run` takes from it, by
/// the same probe and with the same text (T-680).
fn podnet(a: &Args, p: &Path) -> Result<String, String> {
    if !p.is_file() {
        return Err(format!("{}: not a file", p.display()));
    }
    let Some(binary) = &a.podvm else { return Ok(p.display().to_string()) };
    let images = a.images.clone().unwrap_or_default();
    // The probe of podvm has its own row.
    let Ok(unit) = podrunner_podvm::PodvmUnit::new(Podvm { binary: binary.clone() }, &std::env::temp_dir(), &images)
    else {
        return Ok(format!("{}: the hold is not probed: podvm does not answer", p.display()));
    };
    super::routes::podnet_hold(unit, p)
        .map(|_| format!("{}: the seccomp hold (advisory)", p.display()))
        .map_err(|(_, why)| why)
}

/// A directory that exists and takes files, or that can be made (its
/// nearest existing parent takes files); nothing is made.
pub(super) fn writable(d: &Path) -> Result<String, String> {
    let near = d.ancestors().find(|p| p.exists()).unwrap_or(Path::new("/"));
    let probe = near.join(format!(".podrunner-doctor-{}", std::process::id()));
    match std::fs::create_dir(&probe) {
        Ok(()) if near.is_dir() => {
            let _ = std::fs::remove_dir(&probe);
            Ok(d.display().to_string())
        }
        Ok(()) => Err(format!("{}: {} is not a directory", d.display(), near.display())),
        Err(e) => Err(format!("{}: {}: {e}; fix: give a directory that you can write", d.display(), near.display())),
    }
}

/// The rung of `podvm system info`.
fn rung(info: &str) -> Option<String> {
    info.lines().find_map(|l| l.trim().strip_prefix("rung")).and_then(|r| r.split_whitespace().next()).map(String::from)
}

/// The first podvm whose units of the namespace rung have a PID namespace
/// (T-318).
const PIDS_FROM: &str = "0.2.0-beta.30";

/// The line of the rung: a warning where a unit is no boundary for a
/// static binary (T-300), or for the processes of the user (T-318). `info`
/// is the output of `podvm info`, whose first line names the version.
fn rung_text(rung: &str, info: &str) -> String {
    use podrunner_podvm::probe::Version;
    let version = info.split_whitespace().nth(1).and_then(Version::parse);
    let pids = version.is_some_and(|v| Version::parse(PIDS_FROM).is_some_and(|low| v >= low));
    match rung {
        "namespace" if pids => "namespace: each unit has a mount namespace and a PID namespace of its own".into(),
        "namespace" => format!(
            "namespace: each unit has a mount namespace of its own; a job sees and can signal the processes of your user (T-318): take podvm {PIDS_FROM} or later"
        ),
        r => format!(
            "{r}: a static binary of a job can reach the files of your user on this host (T-300); run untrusted jobs with --microvm"
        ),
    }
}

/// The text of the checks: one line for each.
pub(super) fn text(checks: &[Check]) -> String {
    checks.iter().map(|c| format!("{} {}: {}\n", if c.ok { "ok  " } else { "FAIL" }, c.name, c.text)).collect()
}

/// The rows as text, or with `--json` as one object: `ok` and each check
/// (T-342).
pub(super) fn view(checks: &[Check], json: bool) -> String {
    if !json {
        return text(checks);
    }
    let rows: Vec<serde_json::Value> =
        checks.iter().map(|c| serde_json::json!({"name": c.name, "ok": c.ok, "text": c.text})).collect();
    format!("{}\n", serde_json::json!({"ok": checks.iter().all(|c| c.ok), "checks": rows}))
}

#[cfg(test)]
mod rung_tests {
    /// T-300: the doctor reads the rung of podvm and warns off the
    /// namespace rung.
    #[test]
    fn the_rung_of_podvm() {
        let info =
            "podvm 0.2.0-beta.27\n  rung          namespace   (--strict would pass)\n  platform      linux/amd64\n";
        assert_eq!(super::rung(info).as_deref(), Some("namespace"));
        assert!(super::rung_text("userland", info).contains("--microvm"));
        assert!(super::rung_text("namespace", info).contains("take podvm 0.2.0-beta.30"), "T-318");
        let info30 = info.replace("beta.27", "beta.30");
        assert!(super::rung_text("namespace", &info30).contains("a PID namespace of its own"));
        assert_eq!(super::rung("no such line"), None);
    }
}

#[cfg(test)]
mod cache_tests {
    fn sha(b: &[u8]) -> String {
        ring::digest::digest(&ring::digest::SHA256, b).as_ref().iter().map(|b| format!("{b:02x}")).collect()
    }

    /// A ustar entry of `name` with `data`.
    fn entry(name: &str, data: &[u8]) -> Vec<u8> {
        let mut h = vec![0u8; 512];
        h[..name.len()].copy_from_slice(name.as_bytes());
        h[100..108].copy_from_slice(b"0000644\0");
        h[124..136].copy_from_slice(format!("{:011o}\0", data.len()).as_bytes());
        h[136..148].copy_from_slice(b"00000000000\0");
        h[156] = b'0';
        h[257..263].copy_from_slice(b"ustar\0");
        h[263..265].copy_from_slice(b"00");
        h[148..156].copy_from_slice(b"        ");
        let sum: u32 = h.iter().map(|b| u32::from(*b)).sum();
        h[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
        h.extend_from_slice(data);
        h.resize(512 + data.len().div_ceil(512) * 512, 0);
        h
    }

    /// T-630: `doctor --run` names a tar of the image cache that is not
    /// whole, and its fix; a whole tar and no tar pass.
    #[test]
    fn the_doctor_names_a_tar_of_the_cache_that_is_not_whole() {
        let dir = crate::scratch::Scratch::new("t630-doctor");
        let image = format!("runner@sha256:{}", "1".repeat(64));
        assert!(super::cached_tar(&dir, &image).unwrap().contains("no tar of runner@sha256:111"));
        // An image with no digest has no tar in the cache by its name.
        for bare in ["runner:latest", "runner", "runner@sha512:00"] {
            let e = super::cached_tar(&dir, bare).unwrap_err();
            assert!(e.contains(&format!("{bare} names no digest")) && e.contains("NAME@sha256:HEX"), "{e}");
        }
        let (config, layer) = (b"{}".to_vec(), vec![7u8; 3000]);
        let manifest = format!(
            r#"{{"config":{{"digest":"sha256:{}"}},"layers":[{{"digest":"sha256:{}"}}]}}"#,
            sha(&config),
            sha(&layer)
        );
        let index = format!(r#"{{"manifests":[{{"digest":"sha256:{}"}}]}}"#, sha(manifest.as_bytes()));
        let mut tar = Vec::new();
        for blob in [&config[..], &layer[..], manifest.as_bytes()] {
            tar.extend(entry(&format!("blobs/sha256/{}", sha(blob)), blob));
        }
        tar.extend(entry("index.json", index.as_bytes()));
        tar.extend([0u8; 1024]);
        let file = dir.join(format!("{}.tar", "1".repeat(64)));
        std::fs::write(&file, &tar).unwrap();
        assert!(super::cached_tar(&dir, &image).unwrap().ends_with("is whole"));
        std::fs::write(&file, &tar[..2000]).unwrap();
        let e = super::cached_tar(&dir, &image).unwrap_err();
        assert!(e.contains("is not whole (") && e.ends_with(&format!("fix: rm {}", file.display())), "{e}");
    }
}
