use std::cell::RefCell;
use std::collections::BTreeMap;

use super::*;

/// A registry in memory: each tag and its digest; it counts its pulls.
#[derive(Default)]
struct Fake {
    tags: RefCell<BTreeMap<String, String>>,
    pulls: RefCell<u32>,
}

impl ImageSource for Fake {
    fn pull(&self, image: &str) -> Result<String, String> {
        *self.pulls.borrow_mut() += 1;
        self.tags.borrow().get(image).cloned().ok_or_else(|| format!("no {image}"))
    }
}

fn d(c: char) -> String {
    format!("sha256:{}", c.to_string().repeat(64))
}

#[test]
fn the_images_of_a_workflow_that_a_tag_names() {
    let text = "on: push\njobs:\n  a:\n    runs-on: x\n    container: node:22\n    services:\n      db:\n        image: redis\n      pinned:\n        image: r@sha256:00\n    steps:\n      - uses: docker://alpine:3.21\n      - uses: docker://registry.example:5000/team/tool:1\n";
    let w = pod_actions_workflow::parse(text).unwrap().workflow;
    let got = workflow_images(&w).unwrap();
    assert_eq!(got, ["alpine:3.21", "node:22", "redis:latest", "registry.example:5000/team/tool:1"]);
    let bad = "on: push\njobs:\n  a:\n    runs-on: x\n    container: ${{ matrix.image }}\n    steps: [{run: x}]\n";
    let w = pod_actions_workflow::parse(bad).unwrap().workflow;
    assert!(workflow_images(&w).unwrap_err().contains("expression"));
}

#[test]
fn a_tag_is_pinned_by_the_lock_and_a_moved_tag_fails_by_name() {
    let f = Fake::default();
    f.tags.borrow_mut().insert("redis:latest".into(), d('a'));
    f.tags.borrow_mut().insert("registry.example:5000/tool:1".into(), d('b'));
    let images = vec!["redis:latest".to_string(), "registry.example:5000/tool:1".to_string()];
    let mut lock = Lock::default();
    let (pins, changed) = resolve(&images, &mut lock, &f, Mode::Online, "L").unwrap();
    assert!(changed);
    assert_eq!(pins[0], ("docker://redis:latest".to_string(), format!("redis@{}", d('a'))));
    assert_eq!(pins[1].1, format!("registry.example:5000/tool@{}", d('b')), "the port of a registry stays");
    let (_, changed) = resolve(&images, &mut lock, &f, Mode::Frozen, "L").unwrap();
    assert!(!changed);
    let pulls = *f.pulls.borrow();
    resolve(&images, &mut lock, &f, Mode::Offline, "L").unwrap();
    assert_eq!(*f.pulls.borrow(), pulls, "offline pulls nothing");
    f.tags.borrow_mut().insert("redis:latest".into(), d('c'));
    let e = resolve(&images, &mut lock, &f, Mode::Online, "L").unwrap_err();
    assert!(e.contains("`redis:latest` moved") && e.contains(&d('c')), "{e}");
    let e = resolve(&["new:1".to_string()], &mut lock, &f, Mode::Frozen, "L").unwrap_err();
    assert!(e.contains("--frozen"), "{e}");
    let e = resolve(&["new:1".to_string()], &mut lock, &f, Mode::Offline, "L").unwrap_err();
    assert!(e.contains("--offline"), "{e}");
}
