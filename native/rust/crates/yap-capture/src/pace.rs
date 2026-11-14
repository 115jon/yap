//! Frame pacing: WGC delivers at the compositor's rate; the share wants its
//! preset's frame rate.

const HNS_PER_SEC: i64 = 10_000_000;

/// One frame interval in 100 ns units; 0 (no pacing) for 0 fps.
pub(crate) fn interval_hns(fps: u32) -> i64 {
    HNS_PER_SEC.checked_div(i64::from(fps)).unwrap_or(0)
}

/// WGC's own floor between frames (`MinUpdateInterval`, Windows 11 24H2 and
/// later), in 100 ns units: 3/4 of the frame interval, not the interval
/// itself. DWM composes on vsync, so a floor of exactly 1/fps makes every
/// frame that lands a little early wait for the next vsync (measured: 24.6
/// fps for a 30 fps share on a 60 Hz display). The pacer holds the exact
/// rate.
pub(crate) fn min_update_floor_hns(fps: u32) -> i64 {
    interval_hns(fps) * 3 / 4
}

/// Drops WGC frames that arrive faster than the share's frame rate.
///
/// Before Windows 11 24H2 WGC has no rate setting and delivers at the
/// compositor's cadence (about 50-60 fps for a game window), so a 30 fps
/// share captured and encoded about 49 fps. The pacer keeps a running
/// deadline (`next_due += interval`) rather than "one interval since the
/// last admitted frame", so a source whose rate is not a multiple of the
/// target still averages the target rate instead of rounding down to an
/// even divisor.
#[derive(Debug, Default)]
pub(crate) struct FramePacer {
    next_due_hns: Option<i64>,
}

impl FramePacer {
    /// Whether to forward a frame stamped `pts_hns` (100 ns units) when the
    /// share wants one frame per `interval_hns`. `interval_hns <= 0` admits
    /// everything.
    pub(crate) fn admit(&mut self, pts_hns: i64, interval_hns: i64) -> bool {
        if interval_hns <= 0 {
            self.next_due_hns = None;
            return true;
        }
        // Absorb compositor jitter, so a source at exactly the target rate is
        // never halved by a frame that lands slightly early.
        let tolerance = interval_hns / 8;
        match self.next_due_hns {
            // Early, but not so early that the clock must have gone backwards.
            Some(due) if pts_hns + tolerance < due && pts_hns + 2 * interval_hns >= due => false,
            // Due, and less than one whole interval late.
            Some(due) if pts_hns + tolerance >= due && pts_hns < due + interval_hns => {
                self.next_due_hns = Some(due + interval_hns);
                true
            }
            // First frame, a gap longer than one interval (restart instead of
            // bursting to catch up), or a timestamp that went backwards.
            _ => {
                self.next_due_hns = Some(pts_hns + interval_hns);
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Feed `count` frames at `source_fps` (alternating +/- `jitter` hns)
    /// through a pacer targeting `target_fps`, and count the admitted ones.
    fn admitted(source_fps: i64, target_fps: i64, count: i64, jitter: i64) -> i64 {
        let mut pacer = FramePacer::default();
        let interval = if target_fps > 0 {
            HNS_PER_SEC / target_fps
        } else {
            0
        };
        (0..count)
            .filter(|&i| {
                let wobble = if i % 2 == 0 { jitter } else { -jitter };
                pacer.admit(i * HNS_PER_SEC / source_fps + wobble, interval)
            })
            .count() as i64
    }

    #[test]
    fn a_49_fps_source_is_paced_to_30() {
        let n = admitted(49, 30, 49 * 10, 0);
        assert!((295..=305).contains(&n), "admitted {n} of 490 over 10 s");
    }

    #[test]
    fn a_jittery_60_fps_source_is_halved_to_30() {
        let n = admitted(60, 30, 600, 10_000); // +/- 1 ms
        assert!((298..=302).contains(&n), "admitted {n}");
    }

    #[test]
    fn a_source_at_the_target_rate_keeps_every_frame() {
        assert_eq!(admitted(60, 60, 600, 10_000), 600);
        assert_eq!(admitted(30, 30, 300, 20_000), 300);
    }

    #[test]
    fn a_144_hz_source_averages_60() {
        let n = admitted(144, 60, 1440, 0);
        assert!((598..=602).contains(&n), "admitted {n}");
    }

    #[test]
    fn a_slower_source_is_never_dropped() {
        assert_eq!(admitted(24, 30, 240, 0), 240);
    }

    #[test]
    fn a_gap_restarts_the_schedule_without_a_burst() {
        let mut pacer = FramePacer::default();
        let interval = HNS_PER_SEC / 30;
        assert!(pacer.admit(0, interval));
        // One second of static content, then 60 fps again: the first frame is
        // admitted, the one 1/60 s later is not.
        assert!(pacer.admit(HNS_PER_SEC, interval));
        assert!(!pacer.admit(HNS_PER_SEC + HNS_PER_SEC / 60, interval));
        assert!(pacer.admit(HNS_PER_SEC + 2 * HNS_PER_SEC / 60, interval));
    }

    #[test]
    fn backwards_timestamps_restart_the_schedule() {
        let mut pacer = FramePacer::default();
        let interval = HNS_PER_SEC / 30;
        assert!(pacer.admit(10 * HNS_PER_SEC, interval));
        assert!(pacer.admit(HNS_PER_SEC, interval));
    }

    #[test]
    fn zero_interval_admits_everything() {
        assert_eq!(admitted(240, 0, 100, 0), 100);
    }

    #[test]
    fn the_interval_is_one_over_fps() {
        assert_eq!(interval_hns(30), 333_333);
    }

    #[test]
    fn zero_fps_means_no_pacing() {
        assert_eq!(interval_hns(0), 0);
    }

    #[test]
    fn the_wgc_floor_is_three_quarters_of_the_interval() {
        assert_eq!(min_update_floor_hns(50), 150_000);
    }
}
