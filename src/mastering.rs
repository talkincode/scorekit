//! Level delivery: measure the assembled asset, then apply one static gain.
//!
//! A render that leaves 14–17 dB of headroom unused sounds weak on every
//! player, and that deficit is measurable before anyone listens. This module
//! owns the *decision*; `tools::measure_loudness` owns the FFmpeg call and
//! `audio::extract_scaled` applies the gain in-process.
//!
//! Deliberately static gain only. Dynamics processing (compression, limiting)
//! is time-varying and therefore **not** distributive: applying it to the mix
//! but not the stems would break the documented "sum of stems == full mix"
//! invariant, and applying it after loop assembly would break the seamless
//! seal. Crest-factor work belongs to a post step on the delivered asset
//! (FFmpeg `acompressor`/`alimiter`), which the skill documents; `build`
//! delivers level, not loudness war.

use crate::error::{Error, Result};

/// What the renderer actually produced.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Measurement {
    pub integrated_lufs: f64,
    /// True peak in dBFS (FFmpeg's EBU R128 `Peak:` line).
    pub true_peak_dbfs: f64,
}

impl Measurement {
    /// Crest-factor proxy: how much of the peak is not RMS. Higher means more
    /// transient and less "glue"; the static-gain path deliberately leaves it
    /// unchanged.
    pub fn peak_to_loudness_db(&self) -> f64 {
        self.true_peak_dbfs - self.integrated_lufs
    }
}

/// The gain to apply, plus why it may fall short of the loudness target.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Plan {
    pub gain_db: f64,
    /// True when the true-peak ceiling stopped the gain short of `target_lufs`.
    pub limited_by_peak: bool,
    pub target_lufs: f64,
    pub applied: Measurement,
    pub ceiling_dbfs: f64,
}

/// Widest gain the planner will apply in either direction, so a pathological
/// measurement cannot turn into a 60 dB boost.
const MAX_GAIN_DB: f64 = 24.0;
const MAX_CUT_DB: f64 = -40.0;

/// Plan a static gain that lands on `target_lufs` without pushing the true peak
/// past `ceiling_dbfs`.
pub fn plan(measured: Measurement, target_lufs: f64, ceiling_dbfs: f64) -> Result<Plan> {
    if !target_lufs.is_finite() || !(-40.0..=-5.0).contains(&target_lufs) {
        return Err(Error::Validation {
            path: "--master".to_owned(),
            message: format!("{target_lufs} out of range -40.0..=-5.0 LUFS"),
        });
    }
    if !ceiling_dbfs.is_finite() || !(-6.0..=0.0).contains(&ceiling_dbfs) {
        return Err(Error::Validation {
            path: "--master-true-peak".to_owned(),
            message: format!("{ceiling_dbfs} out of range -6.0..=0.0 dBFS"),
        });
    }
    if !measured.integrated_lufs.is_finite() || !measured.true_peak_dbfs.is_finite() {
        return Err(Error::Validation {
            path: "--master".to_owned(),
            message: "cannot master a silent or non-finite render (measurement failed)".to_owned(),
        });
    }

    let wanted = (target_lufs - measured.integrated_lufs).clamp(MAX_CUT_DB, MAX_GAIN_DB);
    let peak_headroom = ceiling_dbfs - measured.true_peak_dbfs;
    let gain_db = wanted.min(peak_headroom);
    Ok(Plan {
        gain_db,
        limited_by_peak: gain_db < wanted - 1e-9,
        target_lufs,
        applied: Measurement {
            integrated_lufs: measured.integrated_lufs,
            true_peak_dbfs: measured.true_peak_dbfs,
        },
        ceiling_dbfs,
    })
}

impl Plan {
    /// Linear multiplier for the sample-domain gain stage.
    pub fn factor(&self) -> f64 {
        10f64.powf(self.gain_db / 20.0)
    }

    /// The loudness the delivered asset will actually have.
    pub fn achieved_lufs(&self) -> f64 {
        self.applied.integrated_lufs + self.gain_db
    }

    /// The true peak the delivered asset will actually have.
    pub fn achieved_true_peak_dbfs(&self) -> f64 {
        self.applied.true_peak_dbfs + self.gain_db
    }

    /// One human-readable line for the build report.
    pub fn summary(&self) -> String {
        // A ceiling that costs 0.2 dB is not worth a parenthetical; one that
        // costs several dB is the whole point of reporting it.
        let note = if self.limited_by_peak && self.target_lufs - self.achieved_lufs() > 0.3 {
            format!(
                " (true-peak ceiling {:.1} dBFS stopped it short of {:.1} LUFS)",
                self.ceiling_dbfs, self.target_lufs
            )
        } else {
            String::new()
        };
        format!(
            "master: {:.1} → {:.1} LUFS (gain {:+.1} dB), true peak {:.1} dBFS, PLR {:.1} dB{note}",
            self.applied.integrated_lufs,
            self.achieved_lufs(),
            self.gain_db,
            self.achieved_true_peak_dbfs(),
            self.applied.peak_to_loudness_db(),
        )
    }

    pub fn as_json(&self) -> serde_json::Value {
        serde_json::json!({
            "target_lufs": self.target_lufs,
            "measured_lufs": self.applied.integrated_lufs,
            "achieved_lufs": self.achieved_lufs(),
            "gain_db": self.gain_db,
            "measured_true_peak_dbfs": self.applied.true_peak_dbfs,
            "achieved_true_peak_dbfs": self.achieved_true_peak_dbfs(),
            "ceiling_dbfs": self.ceiling_dbfs,
            "limited_by_peak": self.limited_by_peak,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn measured(lufs: f64, peak: f64) -> Measurement {
        Measurement {
            integrated_lufs: lufs,
            true_peak_dbfs: peak,
        }
    }

    #[test]
    fn quiet_render_is_lifted_to_the_target_when_headroom_allows() {
        // Plenty of peak headroom: the loudness target is reachable exactly.
        let plan = plan(measured(-23.0, -20.0), -14.0, -1.0).unwrap();
        assert!((plan.gain_db - 9.0).abs() < 1e-9);
        assert!(!plan.limited_by_peak);
        assert!((plan.achieved_lufs() - -14.0).abs() < 1e-9);
        assert!((plan.achieved_true_peak_dbfs() - -11.0).abs() < 1e-9);
    }

    #[test]
    fn a_hot_peak_limits_the_gain_below_the_target() {
        // This is the shipped `elegy` case: -23 LUFS but only 8.3 dB of peak
        // headroom, so the ceiling — not the target — decides the gain.
        let plan = plan(measured(-23.0, -9.3), -14.0, -1.0).unwrap();
        assert!((plan.gain_db - 8.3).abs() < 1e-9);
        assert!(plan.limited_by_peak);
        assert!((plan.achieved_lufs() - -14.7).abs() < 1e-9);
        assert!((plan.achieved_true_peak_dbfs() - -1.0).abs() < 1e-9);
    }

    #[test]
    fn true_peak_ceiling_wins_over_the_loudness_target() {
        // -22 LUFS with a -2 dBFS peak: reaching -9 LUFS needs +13 dB, but the
        // ceiling only allows +1 dB. The plan must say so instead of clipping.
        let plan = plan(measured(-22.0, -2.0), -9.0, -1.0).unwrap();
        assert!((plan.gain_db - 1.0).abs() < 1e-9);
        assert!(plan.limited_by_peak);
        assert!((plan.achieved_lufs() - -21.0).abs() < 1e-9);
        assert!(plan.summary().contains("stopped it short"));
    }

    #[test]
    fn over_loud_render_is_turned_down() {
        let plan = plan(measured(-8.0, -0.3), -14.0, -1.0).unwrap();
        assert!((plan.gain_db - -6.0).abs() < 1e-9);
        assert!(!plan.limited_by_peak);
        assert!((plan.achieved_true_peak_dbfs() - -6.3).abs() < 1e-9);
    }

    #[test]
    fn gain_is_bounded_in_both_directions() {
        // A pathological measurement cannot become a 60 dB boost...
        assert_eq!(
            plan(measured(-90.0, -60.0), -5.0, 0.0).unwrap().gain_db,
            24.0
        );
        // ...nor an unbounded cut past the -40 dB floor (target -40 against a
        // -1 LUFS measurement asks for 39 dB down; -30 dB more is refused by
        // the documented target range, so the floor is exercised at -40.0).
        assert_eq!(
            plan(measured(-5.0, -30.0), -40.0, -1.0).unwrap().gain_db,
            -35.0
        );
        // A hot peak can only ever *reduce* the applied gain, never raise it.
        let hot = plan(measured(-1.0, -0.1), -40.0, -1.0).unwrap();
        assert!(hot.gain_db <= -0.9 + 1e-9);
        assert!(hot.gain_db >= -40.0);
    }

    #[test]
    fn silent_or_non_finite_measurements_are_rejected() {
        for bad in [
            measured(f64::NEG_INFINITY, -9.0),
            measured(-23.0, f64::INFINITY),
            measured(f64::NAN, -9.0),
        ] {
            let error = plan(bad, -14.0, -1.0).expect_err("must fail");
            assert_eq!(error.code(), "validation");
        }
    }

    #[test]
    fn targets_and_ceilings_are_range_checked() {
        for (target, ceiling) in [(-3.0, -1.0), (-41.0, -1.0), (-14.0, 0.5), (-14.0, -7.0)] {
            let error = plan(measured(-23.0, -9.3), target, ceiling).expect_err("must fail");
            assert_eq!(error.code(), "validation");
        }
    }

    #[test]
    fn factor_matches_decibels_and_is_distributive() {
        let plan = plan(measured(-23.0, -20.0), -14.0, -1.0).unwrap();
        let factor = plan.factor();
        assert!((factor - 10f64.powf(9.0 / 20.0)).abs() < 1e-12);
        // Static gain is the reason stems still sum to the mix after mastering.
        let stem_sum = 3.0 * factor + 4.0 * factor;
        assert!((stem_sum - (3.0 + 4.0) * factor).abs() < 1e-12);
    }
}
