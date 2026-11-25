use std::cell::{Cell, RefCell};

use super::*;

/// A host of one process, pid 7, that the kill does not end at once: it
/// leaves the list after the first round, and its stat is what `state`
/// gives for each round. The wait stops after `rounds` rounds.
fn run(rounds: u32, state: impl Fn(u32) -> Option<Stat>) -> (u64, u32, Vec<u32>) {
    let round = Cell::new(0u32);
    let killed = RefCell::new(Vec::new());
    let left = kill_wait(Probe {
        list: || if round.get() == 0 { vec![7] } else { Vec::new() },
        stat: |pid| state(pid).filter(|_| pid == 7),
        kill: |pid| killed.borrow_mut().push(pid),
        ended: || round.get() >= rounds,
        pause: || round.set(round.get() + 1),
    });
    (left, round.get(), killed.into_inner())
}

fn at(round: &Cell<u32>, from: u32, before: Stat, after: Option<Stat>) -> Option<Stat> {
    if round.get() < from { Some(before) } else { after }
}

const RUNS: Stat = Stat { state: 'R', start: 100 };

/// A killed process that frees its memory map leaves the list but runs:
/// the wait goes on to the limit, and the process counts as left.
#[test]
fn a_signalled_process_that_leaves_the_list_but_runs_counts_at_the_limit() {
    let (left, rounds, killed) = run(5, |_| Some(RUNS));
    assert_eq!(killed, vec![7]);
    assert_eq!(rounds, 5, "the wait goes on to the limit");
    assert_eq!(left, 1);
}

/// The wait ends when the signalled process is a zombie or gone.
#[test]
fn the_wait_ends_when_the_signalled_process_ends() {
    let zombie = Stat { state: 'Z', start: 100 };
    for after in [Some(zombie), None] {
        let round = Cell::new(0);
        let l = RefCell::new(0u32);
        let left = kill_wait(Probe {
            list: || if l.replace(1) == 0 { vec![7] } else { Vec::new() },
            stat: |_| at(&round, 3, RUNS, after),
            kill: |_| {},
            ended: || round.get() >= 50,
            pause: || round.set(round.get() + 1),
        });
        assert_eq!(left, 0, "{after:?}");
        assert_eq!(round.get(), 3, "the wait ends at the end of the process: {after:?}");
    }
}

/// A pid that another process took (a new start time) is not the
/// signalled process.
#[test]
fn a_reused_pid_is_not_the_signalled_process() {
    let round = Cell::new(0);
    let l = RefCell::new(0u32);
    let left = kill_wait(Probe {
        list: || if l.replace(1) == 0 { vec![7] } else { Vec::new() },
        stat: |_| at(&round, 2, RUNS, Some(Stat { state: 'S', start: 900 })),
        kill: |_| {},
        ended: || round.get() >= 50,
        pause: || round.set(round.get() + 1),
    });
    assert_eq!((left, round.get()), (0, 2));
}

/// No limit: the count is the list, and no process gets a signal.
#[test]
fn a_zero_limit_counts_and_kills_nothing() {
    let (left, rounds, killed) = run(0, |_| Some(RUNS));
    assert_eq!((left, rounds), (1, 0));
    assert!(killed.is_empty());
}

#[test]
fn the_stat_of_a_name_with_a_parenthesis_and_of_this_process() {
    let fields: Vec<String> = (4..=22).map(|n| n.to_string()).collect();
    let line = format!("42 (a) b (c) S {} 23 24", fields.join(" "));
    assert_eq!(parse_stat(&line), Some(Stat { state: 'S', start: 22 }));
    assert_eq!(parse_stat("42 (a"), None);
    let me = stat(std::process::id()).expect("this process");
    assert!(me.state == 'R' || me.state == 'S', "{me:?}");
    assert!(me.start > 0);
    assert_eq!(stat(u32::MAX), None);
}
