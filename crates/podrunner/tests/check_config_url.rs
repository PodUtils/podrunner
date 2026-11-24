//! `podrunner check` of the `config_url` and `api_url` of a
//! `github-scaleset` source (call C3, T-504): the forms that a scale set
//! serves, and the API that actions/scaleset derives.

use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_podrunner");

/// A temporary directory that goes at the drop, also on a panic.
struct Dir(std::path::PathBuf);

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn dir(name: &str) -> Dir {
    let d = std::env::temp_dir().join(format!("podrunner-check-url-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    Dir(d)
}

/// `check` of a `github-scaleset` source with `config_url` and the extra
/// keys; `ghes` sets `GITHUB_ACTIONS_FORCE_GHES` in its environment.
fn check_scaleset(d: &std::path::Path, i: usize, url: &str, keys: &str, ghes: bool) -> (Option<i32>, String) {
    use std::os::unix::fs::PermissionsExt;
    let token = d.join("user.token");
    std::fs::write(&token, "made-up-token\n").unwrap();
    std::fs::set_permissions(&token, std::fs::Permissions::from_mode(0o600)).unwrap();
    let arch = std::env::consts::ARCH;
    let file = d.join(format!("{i}.toml"));
    std::fs::write(
        &file,
        format!(
            "host = \"h\"\nallow_fake = true\n[[source]]\nname = \"s\"\nkind = \"github-scaleset\"\n\
             tier = \"trusted\"\narch = \"{arch}\"\nsidecar = \"/bin/true\"\nconfig_url = {url:?}\n\
             scale_set = \"s\"\ntoken_file = \"{}\"\n{keys}[unit.fake]\nslots = 1\nclaim = \"confined\"\n\
             [trust]\nusers = [{{ login = \"alice\" }}]\n[pin]\n",
            token.display()
        ),
    )
    .unwrap();
    let mut cmd = Command::new(BIN);
    cmd.args(["check", "--config"]).arg(&file).env_remove("GITHUB_ACTIONS_FORCE_GHES");
    if ghes {
        cmd.env("GITHUB_ACTIONS_FORCE_GHES", "1");
    }
    let out = cmd.output().unwrap();
    (out.status.code(), String::from_utf8_lossy(&out.stderr).into_owned())
}

/// The T-504 review: `check` refuses a `config_url` that no scale set
/// serves (78), and an `api_url` that is not the API that actions/scaleset
/// derives from `config_url`: the sidecar sends its lookups there.
#[test]
fn check_refuses_a_config_url_that_no_scale_set_serves() {
    let d = dir("config-url");
    let personal = "allow_personal_account = true\n";
    let cases = [
        ("https://github.com/alice/app.git", personal, Some(".git")),
        ("https://github.com/alice/APP.GIT", personal, Some(".git")),
        ("https://github.com/alice/app?tab=x", personal, Some("no query")),
        ("https://github.com/alice/app#readme", personal, Some("no query")),
        ("https://github.com/o?x=1", "", Some("no query")),
        ("https://user@github.com/o", "", Some("no query")),
        ("https://github.com/alice", personal, Some("allow_personal_account needs the URL of a repository")),
        ("https://github.com/enterprises/e", "", Some("not the URL of an organization or of a repository")),
        ("https://github.com/alice/app/extra", personal, Some("not the URL of an organization or of a repository")),
        ("https://github.com/alice/..", personal, Some("not the URL of an organization or of a repository")),
        ("https://github.com/al%2Fice/app", personal, Some("not the URL of an organization or of a repository")),
        ("https://github.com/alice/${{ github.repository }}", personal, Some("not the URL")),
        ("github.com/o", "", Some("not the URL")),
        ("", "", Some("not the URL")),
        ("https://ghes.example/o", "api_url = \"https://api.github.com\"\n", Some("api_url")),
        ("https://github.com/o", "api_url = \"https://ghes.example/api/v3\"\n", Some("api_url")),
        // The forms that a scale set serves.
        ("https://github.com/alice/app", personal, None),
        ("https://github.com/Alice/app.js/", personal, None),
        ("https://github.com/o", "", None),
        ("https://github.com/alice/app", "", None),
        ("https://ghes.example/o", "api_url = \"https://ghes.example/api/v3/\"\n", None),
        ("https://www.github.com/o", "api_url = \"https://api.github.com\"\n", None),
        ("https://tenant.ghe.com/o", "api_url = \"https://api.tenant.ghe.com\"\n", None),
        // E of the second review: a host of GHES with `_` or an IDN.
        ("https://ghes_corp.example/org", "", None),
        ("https://bücher.example/org", "api_url = \"https://bücher.example/api/v3\"\n", None),
    ];
    for (i, (url, keys, want)) in cases.into_iter().enumerate() {
        let (code, err) = check_scaleset(&d.0, i, url, keys, false);
        match want {
            Some(word) => {
                assert_eq!(code, Some(78), "{url} {keys}: {err}");
                assert!(err.contains(word) && err.contains("config_url"), "{url} {keys}: {err}");
            }
            None => assert_eq!(code, Some(0), "{url} {keys}: {err}"),
        }
    }
}

/// F of the second T-504 review: `check` reads `GITHUB_ACTIONS_FORCE_GHES`
/// from its own environment, as the sidecar and actions/scaleset do: with
/// it, the API of github.com is `https://github.com/api/v3`.
#[test]
fn check_reads_github_actions_force_ghes_from_its_environment() {
    let d = dir("force-ghes");
    let ghes_api = "api_url = \"https://github.com/api/v3\"\n";
    let hosted_api = "api_url = \"https://api.github.com\"\n";
    for (i, (keys, ghes, want)) in
        [(ghes_api, true, 0), (ghes_api, false, 78), (hosted_api, true, 78), (hosted_api, false, 0)]
            .into_iter()
            .enumerate()
    {
        let (code, err) = check_scaleset(&d.0, i, "https://github.com/o", keys, ghes);
        assert_eq!(code, Some(want), "{keys} GHES {ghes}: {err}");
        if want == 78 {
            let api = if ghes { "https://github.com/api/v3" } else { "https://api.github.com" };
            assert!(err.contains(&format!("{api:?}")), "{keys} GHES {ghes}: {err}");
        }
    }
}
