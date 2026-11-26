//! The memory of a guest (T-573, T-655). The root of a guest is an
//! uncompressed `newc` cpio of the image: packed and unpacked are each
//! about the image, and the guest holds both during the unpack. podvm's
//! rule is `mem = 13/4 × N + 512 MiB`, rounded up to 256 MiB (COORD-219,
//! `machine/guest/mem.rs` of podvm). `check` refuses a pin over the bound
//! of the kind with [`largest_image`] and [`job_space`]. The root of a guest stays in its
//! memory after the boot, and each file of the job takes memory too: the
//! `memory` of an offer is the memory of the job beyond its root
//! ([`guest_mem`]), with at least [`JOB_RESERVE`] of root for its files.

use podrunner_unit::limits::{Limits, size_bytes};

use crate::MicrovmUnit;

/// The memory of a guest beside its initramfs, in podvm's rule.
const GUEST_BASE: u64 = 512 << 20;

/// podvm rounds the memory of its rule up to this step.
const FLOOR_STEP: u64 = 256 << 20;

const MIB: u64 = 1 << 20;

/// The free root that a guest must keep for the files of a job after its
/// image (T-689). The root of a guest is in its memory: a job with less
/// fails with ENOSPC (T-688). A user who needs more sets a larger `mem`.
pub const JOB_RESERVE: u64 = 2 << 30;

/// The largest image that a guest of `mem` bytes boots: the bound of the
/// rule, `(mem - 512 MiB) × 4 / 13`.
pub fn largest_image(mem: u64) -> u64 {
    let room = mem.saturating_sub(GUEST_BASE);
    // `room × 4` can overflow: split it at 13, which gives the same floor.
    room / 13 * 4 + room % 13 * 4 / 13
}

/// The memory of a guest that boots an image of `size` bytes, rounded up to
/// a byte: the smallest `mem` whose `largest_image` holds `size`.
pub fn mem_for(size: u64) -> u64 {
    let need = (u128::from(size) * 13).div_ceil(4) + u128::from(GUEST_BASE);
    u64::try_from(need).unwrap_or(u64::MAX)
}

/// The part of `mem` that the root of a guest can fill: the guest script
/// remounts a `rootfs` root at 75 % of the memory (T-688).
pub fn root_cap(mem: u64) -> u64 {
    // `mem × 3` can overflow: split it at 4, which gives the same floor.
    mem / 4 * 3 + mem % 4 * 3 / 4
}

/// The free space of the root of a guest of `mem` bytes for a job after the
/// unpacked image of `size` bytes (T-689): an estimate, the kernel keeps a
/// little of it.
pub fn job_space(mem: u64, size: u64) -> u64 {
    root_cap(mem).saturating_sub(size)
}

/// The smallest `mem` whose root holds `size` bytes of image and `space`
/// bytes for a job: `job_space(mem, size) >= space`.
pub fn mem_for_space(size: u64, space: u64) -> u64 {
    let need = (u128::from(size) + u128::from(space)) * 4;
    u64::try_from(need.div_ceil(3)).unwrap_or(u64::MAX)
}

/// The `--podvm-mem` of a guest. `kind` is the `mem` of the kind, `lesser`
/// the lesser of the kind and the offer in bytes, and `size` the unpacked
/// bytes of the image. The root of the guest is the unpacked image in its
/// memory, so the job gets `lesser` beyond it: the guest gets the largest
/// of `lesser + size`, the boot floor of the image (`mem_for`, rounded up
/// to 256 MiB as podvm does) and the memory whose root keeps
/// [`JOB_RESERVE`] for the files of the job (`mem_for_space`, T-699), in
/// whole MiB, and never more than the kind: placement counts the kind.
pub fn guest_mem(kind: &str, lesser: Option<u64>, size: Option<u64>) -> String {
    // `plan` refuses a `mem` that is not a size.
    let Ok(kind_bytes) = size_bytes(kind) else { return kind.into() };
    let Some(want) = lesser.filter(|w| *w > 0 && *w < kind_bytes) else { return kind.into() };
    // With no size of the image, its boot floor is not known: the `mem` of
    // the kind is the one value that `check` holds the pins against.
    let Some(size) = size else { return kind.into() };
    let boot = mem_for(size).div_ceil(FLOOR_STEP).saturating_mul(FLOOR_STEP);
    let floor = boot.max(mem_for_space(size, JOB_RESERVE));
    let mib = want.saturating_add(size).max(floor).div_ceil(MIB);
    match mib.checked_mul(MIB) {
        Some(bytes) if bytes < kind_bytes => format!("{mib}M"),
        _ => kind.into(),
    }
}

impl MicrovmUnit {
    /// The limits of a guest for an offer: the lesser of the kind and the
    /// offer (T-198). `launch` adds the image to the memory and keeps the
    /// boot floor (T-655); this function does not know the image: a warm guest has
    /// the memory of the kind and fits only an offer with no less. No pids
    /// for a kind with no limit, so an older podvm gets no flag.
    pub(crate) fn applied(&self, job: &Limits) -> Limits {
        let mem = size_bytes(&self.mem).ok();
        let kind = Limits { memory_bytes: mem, cpus: Some(self.cpus), pids: self.pids, disk_bytes: None };
        let job = Limits { memory_bytes: job.memory_bytes.filter(|m| *m > 0), disk_bytes: None, ..*job };
        let l = kind.lesser(&job);
        Limits {
            memory_bytes: mem.and(l.memory_bytes),
            cpus: l.cpus.map(|c| c.max(1)),
            pids: self.pids.and(l.pids),
            disk_bytes: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: u64 = 1 << 30;

    /// An image of 1416 MiB: 13/4 × 1416 + 512 = 5114 MiB, which rounds up
    /// to 5120 MiB, the boot point of podvm's curve.
    const SIZE: u64 = 1416 * MIB;

    #[test]
    fn a_guest_gets_the_floor_of_its_image_up_to_the_kind() {
        let m = |lesser, size| guest_mem("8G", lesser, size);
        assert_eq!(m(Some(GIB), Some(SIZE)), "5120M", "offer and image below the floor: the floor");
        assert_eq!(m(Some(4 * GIB), Some(SIZE)), "5512M", "between the floor and the kind: the offer and the image");
        assert_eq!(m(Some(6 * GIB), Some(SIZE)), "7560M", "the offer is beyond the root");
        assert_eq!(m(Some(6 * GIB + 1), Some(SIZE)), "7561M", "a MiB rounds up");
        assert_eq!(m(Some(7 * GIB), Some(SIZE)), "8G", "offer and image over the kind: the kind");
        assert_eq!(m(Some(8 * GIB), Some(SIZE)), "8G", "the kind: its own text");
        assert_eq!(m(Some(64 * GIB), Some(SIZE)), "8G", "above the kind");
        assert_eq!(m(Some(0), Some(SIZE)), "8G", "an offer of 0 is no limit");
        assert_eq!(m(None, Some(SIZE)), "8G", "no memory in the offer");
        assert_eq!(m(Some(GIB), None), "8G", "no size of the image: the kind");
        assert_eq!(m(Some(u64::MAX), Some(SIZE)), "8G");
        assert_eq!(m(Some(GIB), Some(u64::MAX)), "8G", "a floor at overflow");
        assert_eq!(m(Some(8 * GIB - 1), Some(1)), "8G", "the last byte of the kind");
        assert_eq!(m(Some(8 * GIB - 1), Some(u64::MAX - 1)), "8G", "offer and image at overflow");
        assert_eq!(m(Some(GIB), Some(7 * GIB)), "8G", "a floor over the kind: the kind");
        assert_eq!(m(Some(GIB), Some(0)), "2731M", "an empty image: the reserve of its job over the offer");
        assert_eq!(m(Some(3 * GIB), Some(0)), "3072M", "an empty image: the offer over the reserve");
        assert_eq!(guest_mem("lots", Some(GIB), Some(SIZE)), "lots", "a kind that is not a size");
        assert_eq!(guest_mem("17179869184", Some(GIB), Some(SIZE)), "5120M", "a kind in bytes");
    }

    /// T-689: the space of a job is 75 % of the memory less the image, the
    /// cap of the guest script, with no overflow.
    /// T-699: a small offer on an image below the knee of the two floors
    /// still leaves its job the reserve of T-689.
    #[test]
    fn a_small_offer_keeps_the_reserve_of_its_job() {
        let m = |lesser, size| guest_mem("8G", lesser, size);
        let mib = |s: String| s.trim_end_matches('M').parse::<u64>().unwrap() * MIB;
        // 512 MiB: boot floor 2304M; 4/3 × (512 + 2048) MiB = 3413.33 MiB.
        let size = 512 * MIB;
        assert_eq!(m(Some(GIB), Some(size)), "3414M");
        assert!(job_space(mib(m(Some(GIB), Some(size))), size) >= JOB_RESERVE);
        for size in [0, 1, MIB, 512 * MIB, 1157 * MIB, 1158 * MIB, SIZE, 2055 * MIB] {
            for want in [1, MIB, GIB, 2 * GIB, 3 * GIB] {
                let got = m(Some(want), Some(size));
                if got == "8G" {
                    continue;
                }
                let bytes = mib(got.clone());
                assert!(job_space(bytes, size) >= JOB_RESERVE, "{size} {want}: {got}");
                assert!(bytes >= want + size && bytes >= mem_for(size), "{size} {want}: {got}");
            }
        }
        assert_eq!(m(Some(1), Some(0)), "2731M", "an empty image: the reserve, 4/3 × 2 GiB");
        assert_eq!(m(Some(4 * GIB), Some(512 * MIB)), "4608M", "the offer and the image over both floors");
        assert_eq!(guest_mem("2G", Some(GIB), Some(0)), "2G", "a reserve over the kind: the kind");
    }

    #[test]
    fn the_space_of_a_job_follows_the_cap_of_the_root() {
        assert!(crate::guest::SCRIPT.contains("mount -o remount,size=75% /"), "the cap of root_cap");
        assert_eq!(root_cap(8 * GIB), 6 * GIB);
        assert_eq!(root_cap(7), 5, "rounded down");
        assert_eq!(u128::from(root_cap(u64::MAX)), u128::from(u64::MAX) * 3 / 4, "no overflow");
        assert_eq!(job_space(8 * GIB, 2 * GIB), 4 * GIB);
        assert_eq!(job_space(4 * GIB, 4 * GIB), 0, "an image over the cap: no space");
        for (size, space) in [(0, 2 * GIB), (SIZE, 2 * GIB), (1, 1), (2055 * MIB, 3)] {
            let m = mem_for_space(size, space);
            assert!(job_space(m, size) >= space && job_space(m - 1, size) < space, "{size} {space}: {m}");
        }
        assert_eq!(mem_for_space(u64::MAX, 1), u64::MAX, "no overflow");
    }

    /// T-573: podvm's curve (COORD-219, experiment 470): each initrd in MiB,
    /// the guest memory in MiB where it does not boot, the memory where it
    /// boots, and whether the rule `13/4 × N + 512 MiB` holds it there. The
    /// rule is podvm's estimate and is above four of the five boot points:
    /// it refuses them, and this test asserts the rule, not the boot.
    #[test]
    fn the_largest_image_follows_the_curve_of_podvm() {
        let curve = [
            (263, 768, 1024, false),
            (519, 1536, 1792, false),
            (1031, 3072, 3328, false),
            (1416, 4096, 5120, true),
            (2055, 6400, 6656, false),
        ];
        for (n, no_boot, boots, holds) in curve {
            assert!(n * MIB > largest_image(no_boot * MIB), "{n} MiB at {no_boot} MiB");
            assert_eq!(n * MIB <= largest_image(boots * MIB), holds, "{n} MiB at {boots} MiB");
        }
        // 13/4 × 1416 + 512 = 5114 MiB; 13/4 × 2055 + 512 = 7190.75 MiB.
        assert_eq!(mem_for(1416 * MIB), 5114 * MIB);
        assert_eq!(mem_for(2055 * MIB), 7190 * MIB + 3 * MIB / 4);
        assert_eq!(mem_for(2055 * MIB).div_ceil(GIB), 8, "the rule of 3 gives 7G");
        assert!(2055 * MIB > largest_image(7 * GIB), "the rule of 3 holds it at 7G");
        // The bound of 8G is (8192 - 512) × 4 / 13 MiB, rounded down to a byte.
        let bound = largest_image(8 * GIB);
        assert_eq!(bound, 2_477_865_747);
        assert!(mem_for(bound) <= 8 * GIB && mem_for(bound + 1) > 8 * GIB);
        assert_eq!(largest_image(1 << 20), 0);
        let top = u128::from(u64::MAX - (512 << 20)) * 4 / 13;
        assert_eq!(u128::from(largest_image(u64::MAX)), top, "no overflow");
        assert_eq!(mem_for(u64::MAX), u64::MAX);
    }
}
