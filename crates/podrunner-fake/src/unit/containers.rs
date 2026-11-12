//! The containers of a fake unit: each is a process tree under a shim of
//! its own, in a directory of the unit, with the mounts mapped to their
//! sources in the argv, the environment and the working directory.

use std::fs;
use std::sync::atomic::Ordering;

use podrunner_schema::{ContainerRoute, UnitId};
use podrunner_unit::procs::{self, TAG_VAR};
use podrunner_unit::shim;
use podrunner_unit::{ContainerId, ContainerSpec, ContainerState, ExecSpec, Exit, UnitCode, UnitError, parse_pinned};

use super::{FakeUnit, UnitPlant, host};
use crate::mounts::Mounts;

impl FakeUnit {
    pub(super) fn start_container(&self, unit: &UnitId, spec: &ContainerSpec) -> Result<ContainerId, UnitError> {
        if self.capability.container != ContainerRoute::Hooks && !self.planted(UnitPlant::ContainerWithNoRoute) {
            return Err(UnitError::new(UnitCode::ContainerNotSupported, "the fake unit has no container route"));
        }
        parse_pinned(&spec.image)?;
        let n = self.containers.fetch_add(1, Ordering::Relaxed);
        let id = ContainerId(format!("c-{n}"));
        let (root, runner_root, tag, sockets) =
            self.with_unit(unit, |r| Ok((r.root.clone(), r.runner_root.clone(), r.tag.clone(), r.sockets.clone())))?;
        let dir = root.join("containers").join(&id.0);
        fs::create_dir_all(&dir).map_err(|e| host(e, "a container directory"))?;
        // The fake applies no option and no mount: it records them, one on
        // each line, for the tests of the hooks, and maps each mount target
        // to its source in the argv, the environment and the workdir.
        let record: String = spec
            .mounts
            .iter()
            .map(|m| format!("mount {} {} {}\n", m.source.display(), m.target, if m.read_only { "ro" } else { "rw" }))
            .chain(spec.options.iter().map(|(k, v)| format!("option {k} {v}\n")))
            .collect();
        fs::write(dir.join("spec"), record).map_err(|e| host(e, "the record of a container"))?;
        let mounts = Mounts::of(&spec.mounts, &runner_root);
        let mut env: Vec<(String, String)> = spec.env.iter().map(|(k, v)| (k.clone(), mounts.map(v))).collect();
        let tag = if self.planted(UnitPlant::UntaggedContainer) {
            let other = format!("{tag}-untagged");
            self.tags.lock().expect("the tags").push(other.clone());
            other
        } else {
            tag
        };
        env.push((TAG_VAR.into(), tag));
        if self.planted(UnitPlant::CredentialInContainers)
            && let Some(c) = self.seen.lock().expect("the credential seen").clone()
        {
            env.push(("PODRUNNER_PLANTED_CREDENTIAL".into(), c));
        }
        let argv: Vec<String> = spec.argv.iter().map(|a| mounts.map(a)).collect();
        let cwd = mounts.workdir(spec.workdir.as_deref(), &dir);
        let shimmed = !self.planted(UnitPlant::UntaggedContainer);
        let child =
            self.spawn(&argv, &cwd, &env, Some(&dir.join("log")), shimmed, Some((&root, sockets.as_deref())))?;
        self.with_unit(unit, |r| {
            r.containers.insert(id.clone(), (child, dir, mounts));
            Ok(id)
        })
    }

    pub(super) fn exec_container(
        &self,
        unit: &UnitId,
        container: &ContainerId,
        spec: &ExecSpec,
    ) -> Result<Exit, UnitError> {
        if spec.stdin && !self.planted(UnitPlant::AcceptStdin) {
            return Err(UnitError::new(UnitCode::ContainerOptionRefused, "stdin"));
        }
        let (dir, mounts, tag, root, sockets) = self.with_unit(unit, |r| {
            let (_, dir, mounts) = r
                .containers
                .get(container)
                .ok_or_else(|| UnitError::new(UnitCode::UnknownUnit, container.0.clone()))?;
            Ok((dir.clone(), mounts.clone(), r.tag.clone(), r.root.clone(), r.sockets.clone()))
        })?;
        let mut env: Vec<(String, String)> = spec.env.iter().map(|(k, v)| (k.clone(), mounts.map(v))).collect();
        env.push((TAG_VAR.into(), tag));
        let argv: Vec<String> = spec.argv.iter().map(|a| mounts.map(a)).collect();
        let cwd = mounts.workdir(spec.workdir.as_deref(), &dir);
        let mut child = self.spawn(&argv, &cwd, &env, spec.log.as_deref(), true, Some((&root, sockets.as_deref())))?;
        child.wait().map(procs::exit_of).map_err(|e| host(e, "exec"))
    }

    pub(super) fn logs_container(&self, unit: &UnitId, container: &ContainerId) -> Result<Vec<u8>, UnitError> {
        self.with_unit(unit, |r| {
            let (_, dir, _) = r
                .containers
                .get(container)
                .ok_or_else(|| UnitError::new(UnitCode::UnknownUnit, container.0.clone()))?;
            fs::read(dir.join("log")).map_err(|e| host(e, "the log"))
        })
    }

    pub(super) fn inspect_container(
        &self,
        unit: &UnitId,
        container: &ContainerId,
    ) -> Result<ContainerState, UnitError> {
        self.with_unit(unit, |r| {
            let (c, _, _) = r
                .containers
                .get_mut(container)
                .ok_or_else(|| UnitError::new(UnitCode::UnknownUnit, container.0.clone()))?;
            let exit = c.try_wait().map_err(|e| host(e, "inspect"))?.map(procs::exit_of);
            Ok(ContainerState { running: exit.is_none(), exit })
        })
    }

    pub(super) fn stop_container(&self, unit: &UnitId, container: &ContainerId) -> Result<(), UnitError> {
        self.with_unit(unit, |r| {
            let (mut c, _, _) = r
                .containers
                .remove(container)
                .ok_or_else(|| UnitError::new(UnitCode::UnknownUnit, container.0.clone()))?;
            shim::kill_unit(c.id(), None, super::KILL_LIMIT);
            let _ = c.kill();
            c.wait().map(|_| ()).map_err(|e| host(e, "stop"))
        })
    }
}
