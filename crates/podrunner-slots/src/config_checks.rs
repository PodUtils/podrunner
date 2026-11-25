//! The checks of `podrunner.toml` that span its tables: the init of the
//! kinds and of the pins, the mitigations of DR-4, and the warm units.

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

    /// The warm units (T-487). Each row resolves to a pinned payload that
    /// a unit of its kind runs; the rows of a kind fit its slots, and the
    /// rows of the host fit `max_units`.
    pub(crate) fn check_warm(&self) -> Result<(), String> {
        if let Some(r) = &self.warm_reserve {
            podrunner_unit::limits::size_bytes(r).map_err(|e| format!("warm_reserve: {e}"))?;
        }
        let mut total: u64 = 0;
        for (name, u) in &self.unit {
            if u.warm_max_age_s == Some(0) {
                return Err(format!("[unit.{name}] warm_max_age_s = 0: a warm unit would end at once"));
            }
            let rows = u.warm_rows();
            if rows.is_empty() {
                continue;
            }
            if !matches!(name.as_str(), "fake" | "microvm" | "incus") {
                return Err(format!("[unit.{name}] warm: only the fake, the microvm and the incus kinds (T-656)"));
            }
            if name == "fake" && u.claim.is_none() {
                return Err(format!("[unit.{name}] warm: a shared principal cannot keep warm units"));
            }
            let mut sum: u64 = 0;
            for (i, (label, n)) in rows.iter().enumerate() {
                if label.is_empty() || label.chars().any(char::is_control) {
                    return Err(format!("[unit.{name}] warm: the label {label:?} is empty or has a control character"));
                }
                if rows[..i].iter().any(|(l, _)| l.eq_ignore_ascii_case(label)) {
                    return Err(format!("[unit.{name}] warm: the label {label:?} is there two times (no case)"));
                }
                if !self.warm_payload(label, u.init) {
                    return Err(format!(
                        "[unit.{name}] warm: no [[pin.payload]] with {} serves the label {label:?}",
                        init_name(u.init)
                    ));
                }
                sum += u64::from(*n);
            }
            if sum > u64::from(u.slots) {
                return Err(format!("[unit.{name}] warm: {sum} warm units, over slots = {}", u.slots));
            }
            total += sum;
        }
        if let Some(m) = self.max_units
            && total > u64::from(m)
        {
            return Err(format!("warm: {total} warm units, over max_units = {m}"));
        }
        Ok(())
    }

    /// A pinned payload (not a sandbox) with `init` serves the warm row of
    /// `label`, by the label rule of placement (`*`: a payload with no
    /// label).
    fn warm_payload(&self, label: &str, init: Option<crate::config::UnitInit>) -> bool {
        let labels: Vec<_> = (label != crate::unit_config::ANY_LABEL).then(|| label.to_string()).into_iter().collect();
        let payloads = self.pin.payload.iter().filter(|p| !p.sandbox);
        payloads.clone().any(|p| {
            p.init == init
                && (p.names_label(&labels)
                    || (p.labels.is_empty()
                        && !payloads.clone().any(|other| other.arch == p.arch && other.names_label(&labels))))
        })
    }
}
