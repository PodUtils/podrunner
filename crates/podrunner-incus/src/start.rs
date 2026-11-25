//! The start of one instance: `podvm run -d` with the script of the guest,
//! for a unit of `start` (the credential files on stdin) and for a warm
//! unit (no stdin: the claim gives the files, `warm.rs`).

use std::fs;
use std::io::Write;
use std::process::Stdio;

use podrunner_unit::{StartSpec, UnitCode, UnitDriver, UnitError};

use crate::guest::{self, credential_stream, run_args};
use crate::stderr::tail;
use crate::{IncusUnit, Running, host};

impl IncusUnit {
    /// Start the instance of `spec`; `claimed` is `Some(false)` for a warm
    /// unit, which gets no stdin.
    pub(crate) fn launch(&self, spec: &StartSpec, claimed: Option<bool>) -> Result<(), UnitError> {
        // The self-check of the guest refuses a mount that it does not know.
        if !spec.mounts.is_empty() {
            return Err(UnitError::new(UnitCode::UnitStartFailed, "this kind mounts no host directory (T-138)"));
        }
        let warm = claimed.is_some();
        let mut units = self.units.lock().expect("the units");
        let dir = self.unit_dir(&spec.unit);
        if units.contains_key(&spec.unit) || dir.exists() {
            return Err(UnitError::new(UnitCode::UnitInUse, spec.unit.to_string()));
        }
        let reference = self
            .references
            .lock()
            .expect("the references")
            .get(&spec.image.digest)
            .cloned()
            .ok_or_else(|| UnitError::new(UnitCode::UnitStartFailed, "the image is not prepared"))?;
        let stream = credential_stream(&spec.credential)?;
        let work = dir.join("work");
        guest::make_work(&dir, &work)?;
        let name = self.name(&spec.unit);
        let fail = |why: String| {
            let _ = self.verb(&["rm", "-f"], &name);
            let _ = fs::remove_dir_all(&dir);
            UnitError::new(UnitCode::UnitStartFailed, why)
        };
        let egress = spec.sockets.as_ref().map(|s| s.join(podrunner_unit::EGRESS_SOCKET));
        // Each host path of a -v or a channel of podvm (#275).
        if let Some(e) = [Some(work.as_path()), egress.as_deref()]
            .into_iter()
            .flatten()
            .find_map(|p| podrunner_unit::mounts::host_path(p, UnitCode::UnitStartFailed).err())
        {
            let _ = fs::remove_dir_all(&dir);
            return Err(e);
        }
        // podvm mounts the socket file into the instance, whose root is a
        // mapped id: the file must take its connect. The socket directory
        // of the unit (mode 0700) keeps each other user of the host out.
        if let Some(e) = egress.as_deref().filter(|e| e.exists()) {
            use std::os::unix::fs::PermissionsExt;
            if let Err(why) = fs::set_permissions(e, fs::Permissions::from_mode(0o666)) {
                let _ = fs::remove_dir_all(&dir);
                return Err(host("the egress socket", why));
            }
        }
        // podvm reads the file before `run -d` returns; it goes with the
        // directory of the unit.
        let (env_file, env) = match podrunner_podvm::write_env_file(&self.podvm, &dir, &spec.env) {
            Ok(x) => x,
            Err(e) => {
                let _ = fs::remove_dir_all(&dir);
                return Err(e);
            }
        };
        let args = run_args(
            &guest::Instance {
                name: &name,
                reference: &reference,
                work: &work,
                egress: egress.as_deref(),
                runner_root: &self.cred_root,
                engine: self.engine.as_deref(),
                payload_engine: self.engine_needs_payload() && self.payload_engines.contains(&spec.image.digest),
                init_image: self.init_image,
                limits: &self.applied(&spec.limits),
                stdin: !warm,
            },
            &env_file,
            &env,
            &spec.argv,
        );
        let input = if warm { Stdio::null() } else { Stdio::piped() };
        let mut child = match self.podvm().args(&args).stdin(input).stdout(Stdio::null()).stderr(Stdio::piped()).spawn()
        {
            Ok(c) => c,
            Err(e) => return Err(fail(format!("podvm run: {}", e.kind()))),
        };
        // `run -d` reads all of stdin before it makes the instance.
        let wrote = child.stdin.take().map(|mut i| i.write_all(&stream));
        let out = child.wait_with_output().map_err(|e| fail(format!("podvm run: {}", e.kind())))?;
        if !out.status.success() || !(warm || matches!(wrote, Some(Ok(())))) {
            return Err(fail(format!("podvm run -d: {}", tail(&out.stderr))));
        }
        let kept = Default::default();
        units.insert(spec.unit.clone(), Running { dir, name, killed: false, exit: None, kept, claimed });
        Ok(())
    }
}
