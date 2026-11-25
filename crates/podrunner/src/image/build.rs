//! Build an OCI archive from the pinned runner recipe.

use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use pod_core::failure::Class;
use serde::Deserialize;

use crate::cli::{Out, err, ok, opts};

use super::{arch_of, runner_pin};

const DOCKERFILE: &str = include_str!("../../../../images/runner/Dockerfile");
const FULL_PACKAGES: &str = include_str!("../../../../images/runner/full.txt");
const RUNNER_LICENSE: &str = include_str!("../../../../images/runner/LICENSE.actions-runner");
const START_HOOK: &str = include_str!("../../../../images/runner/podrunner-job-started.sh");
const CONTAINER_HOOK: &str = include_str!("../../../podrunner-hook/relay/podrunner-hook.sh");
const BUILD_SCRIPT: &str = include_str!("../../../../scripts/build-image.sh");
const ENGINE_INSTALL: &str = include_str!("../../../../images/runner/install-engine.sh");
const ENGINE_LICENSE: &str = include_str!("../../../../images/runner/LICENSE.docker-engine");

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Profile {
    #[default]
    Base,
    Full,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Recipe {
    profile: Profile,
    packages: Vec<String>,
}

impl Recipe {
    fn packages(&self) -> Result<String, String> {
        for name in &self.packages {
            if !valid_package(name) {
                return Err(format!("invalid package name {name:?}"));
            }
        }
        let full = match self.profile {
            Profile::Base => "",
            Profile::Full => FULL_PACKAGES,
        };
        Ok(full.split_whitespace().chain(self.packages.iter().map(String::as_str)).collect::<Vec<_>>().join(" "))
    }
}

/// Apt package names. Pass each word to apt as a package name.
fn valid_package(name: &str) -> bool {
    let first = name.bytes().next();
    name.len() >= 2
        && !name.ends_with('-')
        && first.is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        && name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"+.-".contains(&b))
}

struct BuildDirectory(PathBuf);

impl Drop for BuildDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn stage(root: &Path) -> io::Result<()> {
    for (path, text) in [
        ("build-image.sh", BUILD_SCRIPT),
        ("images/runner/Dockerfile", DOCKERFILE),
        ("images/runner/LICENSE.actions-runner", RUNNER_LICENSE),
        ("images/runner/podrunner-job-started.sh", START_HOOK),
        ("images/runner/install-engine.sh", ENGINE_INSTALL),
        ("images/runner/LICENSE.docker-engine", ENGINE_LICENSE),
        ("crates/podrunner-hook/relay/podrunner-hook.sh", CONTAINER_HOOK),
    ] {
        let path = root.join(path);
        fs::create_dir_all(path.parent().expect("a staged file has a parent"))?;
        fs::write(path, text)?;
    }
    Ok(())
}

fn read_recipe(path: &Path) -> Result<Recipe, String> {
    const MAX: u64 = 64 * 1024;
    let mut input = String::new();
    fs::File::open(path)
        .and_then(|file| file.take(MAX + 1).read_to_string(&mut input))
        .map_err(|e| format!("{}: {e}", path.display()))?;
    if input.len() as u64 > MAX {
        return Err(format!("{}: the recipe is larger than {MAX} bytes", path.display()));
    }
    toml::from_str(&input).map_err(|e| format!("{}: {e}", path.display()))
}

fn io_error(e: io::Error) -> Out {
    err(Class::CantCreate, e.to_string())
}

pub(crate) fn run(args: &[String]) -> Result<Out, Out> {
    let options = opts("image build", args)?;
    let recipe_path = options.need("--recipe")?;
    let out = options.need("--out")?;
    let arch = match options.get("--arch") {
        Some(name) => arch_of(name.to_str().ok_or_else(|| err(Class::Usage, "the architecture is not UTF-8"))?),
        None => arch_of(std::env::consts::ARCH),
    }
    .map_err(|e| err(Class::Usage, e))?;
    let recipe = read_recipe(&recipe_path).map_err(|e| err(Class::Config, e))?;
    let packages = recipe.packages().map_err(|e| err(Class::Config, e))?;
    let full = matches!(recipe.profile, Profile::Full);
    match out.symlink_metadata() {
        Ok(_) => return Err(err(Class::CantCreate, format!("{} already exists", out.display()))),
        Err(e) if e.kind() == io::ErrorKind::NotFound => (),
        Err(e) => return Err(io_error(e)),
    }
    let parent = out.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let dir = BuildDirectory(pod_core::files::new_temp_dir(parent, ".podrunner-image").map_err(io_error)?);
    stage(&dir.0).map_err(io_error)?;

    let pin = runner_pin();
    let sha256 = pin.sha256.get(arch).ok_or_else(|| err(Class::Internal, format!("no pin for {arch}")))?;
    let platform = if arch == "x86_64" { "linux/amd64" } else { "linux/arm64" };
    let status = Command::new("sh")
        .current_dir(&dir.0)
        .args(["build-image.sh", "images/runner/Dockerfile", "--platform", platform])
        // The OCI exporter puts the tag in the reference annotation. Podvm
        // imports an OCI annotation of runner as runner:latest.
        .args(["--provenance=false", "--sbom=false", "--tag", "runner:runner"])
        .args(["--metadata-file", "image.json"])
        .args(["--output", "type=oci,dest=runner.tar"])
        .arg("--build-arg")
        .arg(format!("RUNNER_VERSION={}", pin.version))
        .arg("--build-arg")
        .arg(format!("RUNNER_SHA256={sha256}"))
        .arg("--build-arg")
        .arg(format!("EXTRA_PACKAGES={packages}"))
        .arg("--build-arg")
        .arg(format!("INCLUDE_DOCKER_ENGINE={}", u8::from(full)))
        .arg(".")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .status()
        .map_err(|e| err(Class::Unavailable, format!("start image builder: {e}")))?;
    if !status.success() {
        return Err(err(Class::Unavailable, format!("image builder: {status}")));
    }
    let archive = dir.0.join("runner.tar");
    if let Some(why) = podrunner_podvm::tar_fault(&archive) {
        return Err(err(Class::Data, format!("the image builder returned an invalid OCI archive: {why}")));
    }
    let image =
        fs::read(dir.0.join("image.json")).map_err(|e| err(Class::Data, format!("the image build metadata: {e}")))?;
    let metadata: serde_json::Value =
        serde_json::from_slice(&image).map_err(|e| err(Class::Data, format!("the image build metadata: {e}")))?;
    let digest = metadata["containerimage.digest"]
        .as_str()
        .ok_or_else(|| err(Class::Data, "the image build metadata has no digest"))?;
    let reference = format!("runner@{digest}");
    podrunner_unit::parse_pinned(&reference).map_err(|e| err(Class::Data, e.to_string()))?;
    let built_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| err(Class::Internal, e.to_string()))?
        .as_secs();
    fs::File::open(&archive).and_then(|file| file.sync_all()).map_err(io_error)?;
    fs::hard_link(&archive, &out).map_err(io_error)?;
    let mut payload_pin = format!(
        "[[pin.payload]]\narch = \"{arch}\"\nreference = \"{reference}\"\nbuilt_at_s = {built_at}\nrunner_version = \"{}\"\nassignment_hook = true\n",
        pin.version
    );
    if full {
        payload_pin.push_str("container_engine = true\n");
    }
    Ok(ok(payload_pin))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_packages_from_the_full_recipe() {
        let r: Recipe = toml::from_str("profile = 'full'\npackages = ['g++', 'ninja-build']").unwrap();
        let packages = r.packages().unwrap();
        assert!(packages.contains("cmake"));
        assert!(packages.ends_with(" g++ ninja-build"));
    }

    #[test]
    fn rejects_values_that_are_not_package_names() {
        for name in ["", "-f", ".", "..", "../x", "a", "git-", "a b", "a\nb", "${{x}}", "a=x", "Upper"] {
            let r = Recipe { packages: vec![name.into()], ..Recipe::default() };
            assert!(r.packages().is_err(), "{name:?}");
        }
        assert!(toml::from_str::<Recipe>("profile = 'hosted'").is_err());
        assert!(toml::from_str::<Recipe>("command = 'echo hello'").is_err());
    }
}
