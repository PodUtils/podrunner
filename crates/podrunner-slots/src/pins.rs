//! The signed pin list (DR-4, GitHub #47). An operator key (Ed25519, of
//! `pod-identity`) signs the payloads that a host accepts, so that a step
//! that rewrites the configuration cannot add a payload: the host checks
//! the signature at each read of the file.

use pod_identity::{Identity, PublicKey};
use serde::Deserialize;

use crate::config::Pins;

/// The first bytes of the signed message: no signature of this key serves
/// another purpose.
const LABEL: &[u8] = b"podrunner pin list v1\n";

/// The bytes that the key signs: the label, the age limit, and each payload
/// on its own line, in the order of the file.
pub fn message(pins: &Pins) -> Vec<u8> {
    let mut m = LABEL.to_vec();
    m.extend_from_slice(format!("max_age_s {}\n", pins.max_age_s).as_bytes());
    for p in &pins.payload {
        m.extend_from_slice(format!("payload {} {} {}\n", p.arch, p.reference, p.built_at_s).as_bytes());
    }
    m
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex<const N: usize>(s: &str) -> Option<[u8; N]> {
    if s.len() != 2 * N {
        return None;
    }
    let mut out = [0u8; N];
    for (i, o) in out.iter_mut().enumerate() {
        *o = u8::from_str_radix(s.get(2 * i..2 * i + 2)?, 16).ok()?;
    }
    Some(out)
}

/// The `signer` and the `signature` of `pins` with the key of `seed`.
pub fn sign(pins: &Pins, seed: &[u8; 32]) -> (String, String) {
    let id = Identity::from_seed(seed);
    (id.public().hex(), hex(&id.sign(&message(pins))))
}

/// Check the signature of a pin list that names a signer. A list with no
/// signer passes here; `trusted` at `redirect` needs one (the
/// configuration check).
pub fn verify(pins: &Pins) -> Result<(), String> {
    let (signer, signature) = match (&pins.signer, &pins.signature) {
        (None, None) => return Ok(()),
        (Some(k), Some(s)) => (k, s),
        _ => return Err("[pin]: signer and signature go together".into()),
    };
    let key = PublicKey::parse(signer).ok_or("[pin] signer is not an Ed25519 key of 64 hex digits")?;
    let sig = unhex::<64>(signature).ok_or("[pin] signature is not 128 hex digits")?;
    if !key.verifies(&message(pins), &sig) {
        return Err(format!("[pin]: the signature of {} does not match the pin list", key.fingerprint()));
    }
    Ok(())
}

/// The pin list of a configuration file, with no other check: the input
/// of `podrunner pin-sign`, whose file can hold an old signature.
pub fn of_file(text: &str) -> Result<Pins, String> {
    #[derive(serde::Deserialize)]
    struct File {
        pin: Pins,
    }
    toml::from_str::<toml::Table>(text)
        .map_err(|e| e.to_string())
        .and_then(|t| File::deserialize(toml::Value::Table(t)).map_err(|e| e.to_string()))
        .map(|f| f.pin)
}

/// Read a seed file: 64 hex digits, with no access for the group and the
/// others.
pub fn read_seed(path: &std::path::Path) -> Result<[u8; 32], String> {
    use std::os::unix::fs::MetadataExt;
    let meta = std::fs::metadata(path).map_err(|e| format!("the seed file: {}", e.kind()))?;
    if meta.mode() & 0o077 != 0 {
        return Err("the seed file is open to the group or the others: chmod 0600".into());
    }
    let text = std::fs::read_to_string(path).map_err(|e| format!("the seed file: {}", e.kind()))?;
    unhex::<32>(text.trim()).ok_or_else(|| "the seed file is not 64 hex digits".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Payload;

    fn pins() -> Pins {
        Pins {
            max_age_s: 100,
            payload: vec![Payload { arch: "x86_64".into(), reference: "r@sha256:00".into(), built_at_s: 7 }],
            signer: None,
            signature: None,
        }
    }

    #[test]
    fn a_signed_list_verifies_and_each_change_fails() {
        let mut p = pins();
        assert_eq!(verify(&p), Ok(()));
        let (k, s) = sign(&p, &[7; 32]);
        (p.signer, p.signature) = (Some(k.clone()), Some(s.clone()));
        assert_eq!(verify(&p), Ok(()));
        let changes: [fn(&mut Pins); 4] = [
            |p| p.max_age_s += 1,
            |p| p.payload[0].built_at_s += 1,
            |p| p.payload[0].reference.push('1'),
            |p| p.payload.push(p.payload[0].clone()),
        ];
        for change in changes {
            let mut q = p.clone();
            change(&mut q);
            assert!(verify(&q).unwrap_err().contains("does not match"));
        }
        // Another key, a half pair, and bad hex.
        let mut q = p.clone();
        q.signer = Some(sign(&p, &[8; 32]).0);
        assert!(verify(&q).is_err());
        q.signature = None;
        assert!(verify(&q).unwrap_err().contains("go together"));
        let mut q = p;
        q.signature = Some("zz".repeat(64));
        assert!(verify(&q).unwrap_err().contains("128 hex"));
    }
}
