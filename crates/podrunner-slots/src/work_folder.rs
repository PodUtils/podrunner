//! `[unit.KIND] work_folder` (T-541): the work folder of the runner in a
//! unit. The mint gives `.runner` with the folder of the source (`_work`,
//! under the runner root). A unit of the hosted snapshot needs the hosted
//! path, `/home/runner/work`, so that `GITHUB_WORKSPACE` is
//! `/home/runner/work/REPO/REPO` as on a hosted runner. A JIT runner runs no
//! `config.sh`, so `.runner` is the one place that names the folder.

use podrunner_schema::{Credential, Secret};
use serde_json::Value;

/// The file of the runner that names its work folder.
pub(crate) const RUNNER_FILE: &str = ".runner";

/// Set the work folder of the runner in `.runner` of `credential`. The
/// runner reads the key with no case, so each other spelling goes. An
/// error names the cause, never the content of the file.
pub(crate) fn apply(credential: &mut Credential, folder: &str) -> Result<(), String> {
    let file = credential
        .files
        .iter_mut()
        .find(|f| f.name == RUNNER_FILE)
        .ok_or_else(|| format!("the credential has no {RUNNER_FILE} file"))?;
    let text = file.content.expose();
    // The runner reads a file with a byte order mark; keep it.
    let (mark, body) = match text.strip_prefix('\u{feff}') {
        Some(body) => ("\u{feff}", body),
        None => ("", text),
    };
    let mut value: Value = serde_json::from_str(body).map_err(|_| format!("{RUNNER_FILE} is not JSON"))?;
    let map = value.as_object_mut().ok_or_else(|| format!("{RUNNER_FILE} is not a JSON object"))?;
    map.retain(|k, _| !k.eq_ignore_ascii_case("workFolder"));
    map.insert("workFolder".into(), folder.into());
    file.content = Secret::new(format!("{mark}{value}"));
    Ok(())
}

#[cfg(test)]
mod tests {
    use podrunner_schema::CredentialFile;

    use super::*;

    fn cred(runner: &str) -> Credential {
        let file = |name: &str, text: &str| CredentialFile { name: name.into(), content: Secret::new(text) };
        Credential { files: vec![file(".credentials", "{\"scheme\":\"OAuth\"}"), file(RUNNER_FILE, runner)] }
    }

    fn runner(c: &Credential) -> Value {
        let f = c.files.iter().find(|f| f.name == RUNNER_FILE).unwrap();
        serde_json::from_str(f.content.expose().trim_start_matches('\u{feff}')).unwrap()
    }

    #[test]
    fn the_work_folder_replaces_the_one_of_the_mint() {
        let mut c = cred(r#"{"agentId":7,"agentName":"r-1","workFolder":"_work","WorkFolder":"other"}"#);
        apply(&mut c, "/home/runner/work").unwrap();
        let v = runner(&c);
        assert_eq!(v["workFolder"], "/home/runner/work");
        assert_eq!(v.as_object().unwrap().keys().filter(|k| k.eq_ignore_ascii_case("workfolder")).count(), 1);
        assert_eq!((v["agentId"].clone(), v["agentName"].clone()), (7.into(), "r-1".into()));
        // The other files stay as the mint gave them.
        assert_eq!(c.files[0].content.expose(), "{\"scheme\":\"OAuth\"}");
    }

    #[test]
    fn a_file_with_no_folder_or_with_a_byte_order_mark_gets_one() {
        let mut c = cred("\u{feff}{\"agentName\":\"r-2\"}");
        apply(&mut c, "/w").unwrap();
        let text = c.files[1].content.expose();
        assert!(text.starts_with('\u{feff}'), "the mark goes: {text:?}");
        assert_eq!(runner(&c)["workFolder"], "/w");
    }

    #[test]
    fn a_file_that_is_not_an_object_is_refused_with_no_content() {
        for (text, want) in [("not json SECRET", "is not JSON"), ("[\"SECRET\"]", "is not a JSON object")] {
            let e = apply(&mut cred(text), "/w").unwrap_err();
            assert!(e.contains(want) && !e.contains("SECRET"), "{e}");
        }
        let mut none = Credential { files: vec![] };
        assert!(apply(&mut none, "/w").unwrap_err().contains("no .runner file"));
    }
}
