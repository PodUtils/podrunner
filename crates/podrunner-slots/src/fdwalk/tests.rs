use std::fs;
use std::os::unix::fs::symlink;

use super::*;
use crate::tests::TempDir;

const ALL: Opts = Opts { same_dev: true, max_open: MAX_OPEN };

/// The bytes of the regular files that a walk reads, and how it ended.
fn bytes(root: &Path, opts: Opts, mut on_dir: impl FnMut(&CStr)) -> (u64, End, usize) {
    let mut n = 0u64;
    let (end, peak) = walk(
        root,
        opts,
        |st| {
            if st.st_mode & libc::S_IFMT == libc::S_IFREG {
                n += u64::try_from(st.st_size).unwrap();
            }
            true
        },
        |_, name| on_dir(name),
    );
    (n, end, peak)
}

#[test]
fn a_walk_counts_the_files_of_the_tree_and_follows_no_link() {
    let dir = TempDir::new();
    let root = dir.0.join("root");
    fs::create_dir_all(root.join("a/b")).unwrap();
    fs::write(root.join("one"), [0; 3]).unwrap();
    fs::write(root.join("a/b/two"), [0; 5]).unwrap();
    let outside = dir.0.join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("big"), [0; 1000]).unwrap();
    symlink(&outside, root.join("a/link")).unwrap();
    assert_eq!(bytes(&root, ALL, |_| {}), (8, End::Done, 3));
    // A root that is a link, a file or not there: nothing.
    assert_eq!(bytes(&root.join("a/link"), ALL, |_| {}).0, 0);
    assert_eq!(bytes(&root.join("one"), ALL, |_| {}).0, 0);
    assert_eq!(bytes(&root.join("none"), ALL, |_| {}).0, 0);
}

/// T-678: a job swaps a directory that the walk listed for a link to a
/// tree outside, before the walk opens it. The walk does not follow it.
#[test]
fn a_directory_swapped_for_a_link_during_the_walk_is_not_followed() {
    let dir = TempDir::new();
    let root = dir.0.join("root");
    fs::create_dir_all(root.join("x")).unwrap();
    fs::create_dir_all(root.join("y/inner")).unwrap();
    fs::write(root.join("y/inner/file"), [0; 10]).unwrap();
    let outside = dir.0.join("outside");
    fs::create_dir_all(outside.join("inner")).unwrap();
    fs::write(outside.join("inner/big"), [0; 1000]).unwrap();
    let mut swapped = false;
    let (n, end, _) = bytes(&root, ALL, |name| {
        // The first directory after the root is `x` or `y`; swap the other.
        if !swapped && (name.to_bytes() == b"x" || name.to_bytes() == b"y") {
            swapped = true;
            let other = if name.to_bytes() == b"x" { "y" } else { "x" };
            let _ = fs::rename(root.join(other), dir.0.join("moved"));
            symlink(&outside, root.join(other)).unwrap();
        }
    });
    assert!(swapped);
    assert_eq!(end, End::Done);
    assert!(n < 1000, "the walk followed the link out of the tree: {n} bytes");
}

/// T-678: a tree deeper than the bound of descriptors is read in full, and
/// the walk holds at most the bound and one more.
#[test]
fn a_deep_tree_is_read_with_a_bound_of_descriptors() {
    let dir = TempDir::new();
    let root = dir.0.join("root");
    let mut p = root.clone();
    for i in 0..40 {
        p = p.join(format!("d{i}"));
        fs::create_dir_all(&p).unwrap();
        fs::write(p.join("f"), [0; 2]).unwrap();
        // A sibling at each level makes the walk come back up.
        fs::create_dir_all(p.with_file_name(format!("s{i}"))).unwrap();
        fs::write(p.with_file_name(format!("s{i}")).join("g"), [0; 1]).unwrap();
    }
    let (n, end, peak) = bytes(&root, Opts { same_dev: true, max_open: 4 }, |_| {});
    assert_eq!((n, end), (40 * 3, End::Done));
    assert!(peak <= 5, "the walk held {peak} descriptors");
    assert_eq!(bytes(&root, ALL, |_| {}).0, 40 * 3);
}

/// T-678: a job moves a directory that the walk closed; the walk cannot
/// check its way back, and ends `Lost`.
#[test]
fn a_closed_directory_that_moves_ends_the_walk_lost() {
    let dir = TempDir::new();
    let root = dir.0.join("root");
    fs::create_dir_all(root.join("a/b/c/d/e")).unwrap();
    fs::create_dir_all(root.join("a/sibling")).unwrap();
    let mut moved = false;
    let (_, end, _) = bytes(&root, Opts { same_dev: true, max_open: 2 }, |name| {
        if name.to_bytes() == b"e" && !moved {
            moved = true;
            fs::rename(root.join("a/b"), root.join("b-moved")).unwrap();
        }
    });
    assert!(moved);
    assert_eq!(end, End::Lost);
}

#[test]
fn the_file_callback_stops_the_walk() {
    let dir = TempDir::new();
    for i in 0..5 {
        fs::write(dir.0.join(i.to_string()), [0; 1]).unwrap();
    }
    let mut seen = 0;
    let (end, _) = walk(
        &dir.0,
        ALL,
        |_| {
            seen += 1;
            seen < 2
        },
        |_, _| {},
    );
    assert_eq!((end, seen), (End::Stopped, 2));
}

/// The mode of a directory that a job shut comes back through its
/// descriptor, and a link at its name is not changed.
#[test]
fn open_up_gives_the_owner_the_bits_of_the_directory() {
    use std::os::unix::fs::PermissionsExt;
    let dir = TempDir::new();
    let shut = dir.0.join("shut");
    fs::create_dir_all(shut.join("inner")).unwrap();
    fs::set_permissions(&shut, fs::Permissions::from_mode(0o000)).unwrap();
    let (end, _) = walk(&dir.0, ALL, |_| true, |fd, _| open_up_fd(fd));
    assert_eq!(end, End::Done);
    assert_eq!(fs::metadata(&shut).unwrap().permissions().mode() & 0o777, 0o700);
}
