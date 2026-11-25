//! The checks of `podrunner.toml` that span its tables: the init of the
//! kinds and of the pins, and the mitigations of DR-4.

use podrunner_schema::Tier;

use crate::config::{Config, init_name};

impl Config {
    /// `init` of the kinds and of the pins (T-549). Only the incus and the
    /// microvm kinds boot the init of an image: the `podvm` kind runs
    /// `run -d` with `--ulimit` and `--podvm-confine`, which podvm refuses
    /// with an init, and its namespace rung maps one uid. A payload that
    /// needs an init that no kind boots would refuse each job of its labels.
    pub(crate) fn check_init(&self) -> Result<(), String> {
        if let Some((n, _)) =
            self.unit.iter().find(|(n, u)| u.init.is_some() && !matches!(n.as_str(), "incus" | "microvm"))
        {
            return Err(format!("[unit.{n}] init: only the incus and the microvm kinds boot the init of an image"));
        }
        if let Some(p) =
            self.pin.payload.iter().find(|p| p.init.is_some() && !self.unit.values().any(|u| u.init == p.init))
        {
            return Err(format!(
                "[[pin.payload]] {}: {} and no [unit.KIND] boots it: set init = \"image\" on an incus or a microvm kind",
                p.reference,
                init_name(p.init)
            ));
        }
        Ok(())
    }

    /// The mitigations of DR-4 that the file must state when `trusted`
    /// runs at `redirect` (GitHub #4, T-045).
    pub(crate) fn check_redirect(&self) -> Result<(), String> {
        if self.same_principal_ok {
            return Err(
                "[tier.trusted] redirect needs one job at a time on a shared uid: same_principal_ok = false (DR-4)"
                    .into(),
            );
        }
        if self.pin.signer.is_none() {
            return Err("[tier.trusted] redirect needs a signed pin list: [pin] signer and signature (DR-4)".into());
        }
        if self.repository.is_none() {
            return Err(
                "[tier.trusted] redirect needs one repository for the host: repository = \"owner/repo\" (DR-4)".into(),
            );
        }
        if let Some(s) = self.source.iter().find(|s| s.tier == Tier::Trusted && s.slots.is_none()) {
            return Err(format!("[[source]] {:?}: a trusted source at redirect needs slots (DR-4)", s.name));
        }
        Ok(())
    }
}
