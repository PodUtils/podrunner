//! The checks of the configuration that name a fault before `serve`
//! starts: the guest kinds (#283) and the pin payloads (#270, #287).

use super::*;

/// #283: an entry of `guest_kinds` that names no `[unit.NAME]` is refused by
/// name, also in another case.
#[test]
fn a_guest_kind_with_no_unit_is_refused() {
    let unit = "[unit.microvm]\nslots = 1\ncommand = [\"x\"]\n";
    let parse = |kinds: &str| Config::parse(&format!("host = \"h\"\nguest_kinds = {kinds}\n{unit}[pin]\n"));
    let e = parse("[\"microvmm\"]").unwrap_err();
    assert!(e.contains("guest_kinds") && e.contains("microvmm"), "{e}");
    assert!(parse("[\"MicroVM\"]").unwrap_err().contains("MicroVM"));
    assert!(parse("[\"\"]").is_err());
    parse("[\"microvm\"]").unwrap();
    parse("[]").unwrap();
}

/// #287: `check` refuses a duplicate reference, an age limit of 0 and a
/// runner version that is not a version; a payload built after the clock
/// is stale, not young for ever.
#[test]
fn a_bad_pin_payload_is_refused_by_name() {
    let r = format!("reference = \"{PIN}\"");
    let payload = |extra: &str| format!("[[pin.payload]]\narch = \"x86_64\"\n{r}\nbuilt_at_s = {NOW_UNIX}\n{extra}\n");
    let parse = |pin: &str, payloads: &str| Config::parse(&format!("host = \"h\"\n[pin]\n{pin}\n{payloads}"));
    let e = parse("", &format!("{}{}", payload(""), payload(""))).unwrap_err();
    assert!(e.contains("two times") && e.contains(PIN), "{e}");
    // One index digest for two architectures is not a duplicate.
    parse("", &format!("{}{}", payload(""), payload("").replace("x86_64", "aarch64"))).unwrap();
    assert!(parse("max_age_s = 0", &payload("")).unwrap_err().contains("max_age_s"));
    assert!(parse("", &payload("max_age_s = 0")).unwrap_err().contains("max_age_s"));
    for bad in ["latest", "2.338", "2.338.2.1", "v2.338.2", "", "2..2"] {
        let e = parse("", &payload(&format!("runner_version = \"{bad}\""))).unwrap_err();
        assert!(e.contains("runner_version"), "{bad}: {e}");
    }
    // #270: an arch that placement does not know.
    for arch in ["riscv64", "x86-64", "amd64", "", "X86_64"] {
        let e = parse("", &payload("").replace("\"x86_64\"", &format!("{arch:?}"))).unwrap_err();
        assert!(e.contains("is not x86_64 or aarch64"), "{arch}: {e}");
    }
    let ok = parse("", &payload("runner_version = \"2.338.2\"\nmax_age_s = 60")).unwrap();
    // Built one day after the clock: stale; at the clock or one minute
    // after (a skew of the clocks): not stale.
    let p = &ok.pin.payload[0];
    assert!(p.stale(&ok.pin, NOW_UNIX - 86_400));
    assert!(!p.stale(&ok.pin, NOW_UNIX) && !p.stale(&ok.pin, NOW_UNIX - 60));
}

/// T-487: the journal keeps the warm units of the host under the source
/// `@warm`, so a source name that starts with `@`, an empty name, and a
/// name with a control character are refused.
#[test]
fn a_source_name_of_the_host_or_with_a_control_character_is_refused() {
    let parse = |name: &str| {
        // TOML takes DEL only as an escape.
        let name = serde_json::to_string(name).unwrap().replace('\u{7f}', "\\u007f");
        let source = format!("[[source]]\nname = {name}\nkind = \"fake\"\ntier = \"trusted\"\narch = \"x86_64\"\n");
        Config::parse(&format!("host = \"h\"\nallow_fake = true\n{source}[pin]\n"))
    };
    for bad in ["@warm", "@", "", "a\nb", "a\u{7f}", "a\tb"] {
        let e = parse(bad).unwrap_err();
        assert!(e.contains("[[source]]") && e.contains("name"), "{bad:?}: {e}");
    }
    for ok in ["a", "a@b", "github-trusted", "Ünïcode"] {
        parse(ok).unwrap();
    }
}

/// T-487: the keys of warm units. Each row needs a payload that a unit of
/// its kind runs, the rows fit `slots` and `max_units`, and only the fake,
/// the microvm and the incus kinds take them.
#[test]
fn the_warm_keys_accept_pinned_rows_within_their_limits() {
    let pin = |labels: &str, extra: &str| {
        format!(
            "[[pin.payload]]\narch = \"x86_64\"\nreference = \"{PIN}\"\nbuilt_at_s = {NOW_UNIX}\nlabels = {labels}\n{extra}\n"
        )
    };
    let parse = |top: &str, unit: &str, pins: &str| {
        Config::parse(&format!(
            "host = \"h\"\nallow_fake = true\n{top}\n[unit.fake]\nclaim = \"confined\"\n{unit}\n[pin]\n{pins}"
        ))
    };
    let refused = |top: &str, unit: &str, pins: &str, want: &str| {
        let e = parse(top, unit, pins).unwrap_err();
        assert!(e.contains(want), "{unit:?}: {e}");
    };
    let any = pin("[]", "");
    refused("", "slots = 2\nwarm = 3", &any, "over slots = 2");
    refused("max_units = 1", "slots = 2\nwarm = 2", &any, "over max_units = 1");
    refused("", "slots = 2\nwarm = { \"gpu\" = 1 }", &pin("[\"cpu\"]", ""), "serves the label \"gpu\"");
    refused("", "slots = 2\nwarm = 1", &pin("[\"cpu\"]", ""), "serves the label \"*\"");
    refused("", "slots = 2\nwarm = 1", &pin("[]", "sandbox = true"), "no [[pin.payload]]");
    // The one payload boots its own init, which the incus kind boots and
    // the microvm kind does not.
    let init = format!(
        "host = \"h\"\n[unit.incus]\nslots = 1\ninit = \"image\"\n[unit.microvm]\nslots = 2\nwarm = 1\n[pin]\n{}",
        pin("[]", "init = \"image\"")
    );
    let e = Config::parse(&init).unwrap_err();
    assert!(e.contains("[unit.microvm] warm: no [[pin.payload]] with no init"), "{e}");
    let labelled_init = init.replace("warm = 1", "warm = { cpu = 1 }").replace("labels = []", r#"labels = ["cpu"]"#);
    let unlabelled = any.replace(&"1".repeat(64), &"2".repeat(64));
    let e = Config::parse(&format!("{labelled_init}{unlabelled}")).unwrap_err();
    assert!(e.contains("[unit.microvm] warm: no [[pin.payload]] with no init"), "{e}");
    assert!(e.contains(r#"serves the label "cpu""#), "{e}");
    refused("", "slots = 3\nwarm = { \"Cpu\" = 1, \"cpu\" = 1 }", &pin("[\"cpu\"]", ""), "two times");
    refused("", "slots = 3\nwarm = { \"\" = 1 }", &any, "empty");
    refused("", "slots = 3\nwarm = 1\nwarm_max_age_s = 0", &any, "warm_max_age_s = 0");
    refused("warm_reserve = \"1X\"", "slots = 1", &any, "warm_reserve");
    refused("warm_reserve = \"0\"", "slots = 1", &any, "warm_reserve");
    let shared = format!("host = {:?}\nallow_fake = true\n[unit.fake]\nslots = 1\nwarm = 1\n[pin]\n{any}", "h");
    assert!(Config::parse(&shared).unwrap_err().contains("shared principal"));
    assert!(Config::parse(&shared.replace("warm = 1", "warm = 0")).is_ok());
    let podvm = format!("host = \"h\"\n[unit.podvm]\nslots = 2\nwarm = 1\n[pin]\n{any}");
    assert!(Config::parse(&podvm).unwrap_err().contains("T-656"));
    parse("max_units = 2", "slots = 2\nwarm = 2", &any).unwrap();
    let cpu = pin("[\"cpu\"]", "").replace(&"1".repeat(64), &"2".repeat(64));
    parse("", "slots = 3\nwarm = { \"*\" = 1, \"CPU\" = 2 }", &format!("{any}{cpu}")).unwrap();
    parse("warm_reserve = \"512M\"", "slots = 2\nwarm = 0\nwarm_max_age_s = 60", &any).unwrap();
    parse("", "slots = 2\nwarm = { \"cpu\" = 0 }", &any).unwrap();
}
