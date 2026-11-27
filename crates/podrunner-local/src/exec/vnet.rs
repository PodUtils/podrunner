//! The vnet of podnet for a podvm unit (T-220): on a host that refuses each
//! AF_INET bind and connect, the runner of a unit runs under the vnet shim
//! of podnet (`podnet net shim`). The shim maps one loopback address of the
//! unit to the front of its socket directory (`PODNET_VNET_FORWARD`,
//! podnet 0.4.0-beta.14 or later), and the front gives the fake broker on a
//! unix socket and the egress module. The host binds no AF_INET socket.

use std::collections::BTreeMap;
use std::path::Path;

use podrunner_unit::socket::SUN_PATH_MAX;

/// The loopback port of the unit that the shim forwards to the front.
pub const PORT: u16 = 3128;

/// The file name of the shim in the socket directory.
pub const SHIM: &str = "libpodnet-vnet.so";

/// Copy the shim `lib` into the socket directory `dir` of a unit.
pub(super) fn place(lib: &Path, dir: &Path) -> Result<(), String> {
    std::fs::copy(lib, dir.join(SHIM)).map(|_| ()).map_err(|e| format!("the vnet shim {}: {e}", lib.display()))
}

/// The host path of the socket directory `dir` as the shim takes it: the
/// loader splits LD_PRELOAD at a space and a colon, and the shim splits
/// its forwards at a comma, so such a path would leave the runner with no
/// shim and no line (T-561); the egress socket must fit `sun_path`.
pub(super) fn host_dir(dir: &Path) -> Result<String, String> {
    let fix = "set TMPDIR to a short path with no space, comma or colon";
    let s =
        dir.to_str().ok_or_else(|| format!("--vnet: the socket directory {} is not UTF-8; {fix}", dir.display()))?;
    if s.contains([' ', ',', ':']) || s.chars().any(char::is_control) {
        return Err(format!(
            "--vnet: the socket directory {s} has a space, a comma or a colon, which the vnet shim cannot take; {fix}"
        ));
    }
    let sock = format!("{s}/{}", podrunner_unit::EGRESS_SOCKET);
    if sock.len() > SUN_PATH_MAX {
        return Err(format!("--vnet: the socket {sock} is longer than {SUN_PATH_MAX} bytes; {fix}"));
    }
    Ok(s.to_string())
}

/// The environment of a runner under the shim; `mount` is the socket
/// directory in the unit: its host path, which the unit mounts at the same
/// path (T-561). Each request goes through the front as a proxy:
/// the name of the broker is not a loopback name.
pub(super) fn env(env: &mut BTreeMap<String, String>, mount: &str) {
    let proxy = format!("http://127.0.0.1:{PORT}");
    env.insert("LD_PRELOAD".into(), format!("{mount}/{SHIM}"));
    env.insert(
        "PODNET_VNET_FORWARD".into(),
        format!("127.0.0.1:{PORT}=unix:{mount}/{}", podrunner_unit::EGRESS_SOCKET),
    );
    for k in ["HTTP_PROXY", "HTTPS_PROXY", "http_proxy", "https_proxy"] {
        env.insert(k.into(), proxy.clone());
    }
    for k in ["NO_PROXY", "no_proxy"] {
        env.insert(k.into(), String::new());
    }
    // `fetch` of Node reads the proxy variables only with this (Node 22.21
    // and 24.5 on), as the host path sets it (#241).
    env.entry("NODE_USE_ENV_PROXY".into()).or_insert_with(|| "1".into());
}

#[cfg(test)]
mod tests {
    /// T-561, the review: a socket directory that the loader or the shim
    /// would split, or one past `sun_path`, is refused with its fix.
    #[test]
    fn a_socket_directory_that_the_shim_cannot_take_is_refused() {
        use std::path::Path;
        assert_eq!(
            super::host_dir(Path::new("/tmp/podrunner-local-unit-1-u-1")).unwrap(),
            "/tmp/podrunner-local-unit-1-u-1"
        );
        for bad in ["/tmp/my dir/u-1", "/tmp/a,b/u-1", "/tmp/a:b/u-1", "/tmp/a\tb/u-1"] {
            let e = super::host_dir(Path::new(bad)).unwrap_err();
            assert!(e.contains("set TMPDIR"), "{bad}: {e}");
        }
        let long = format!("/tmp/{}", "x".repeat(100));
        assert!(super::host_dir(Path::new(&long)).unwrap_err().contains("longer than 107"));
        let edge = format!("/{}", "x".repeat(super::SUN_PATH_MAX - 1 - 1 - podrunner_unit::EGRESS_SOCKET.len()));
        assert!(super::host_dir(Path::new(&edge)).is_ok(), "a socket of 107 bytes fits");
    }

    /// T-220: the runner of a unit reaches the front through the shim.
    #[test]
    fn the_environment_of_a_runner_under_the_vnet_shim() {
        let mut env = std::collections::BTreeMap::new();
        super::env(&mut env, "/run/podrunner");
        assert_eq!(env["LD_PRELOAD"], "/run/podrunner/libpodnet-vnet.so");
        assert_eq!(env["PODNET_VNET_FORWARD"], "127.0.0.1:3128=unix:/run/podrunner/egress.sock");
        assert_eq!(env["HTTPS_PROXY"], "http://127.0.0.1:3128");
        assert_eq!(env["NO_PROXY"], "");
        assert_eq!(env["NODE_USE_ENV_PROXY"], "1", "#241");
    }
}
