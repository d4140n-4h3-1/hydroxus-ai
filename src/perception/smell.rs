//! Tracking one source by the scent it leaves behind. The game lays samples along the source's
//! path and decides whether scent can carry from a sample to the nose.

use nalgebra::Vector3;
use std::collections::VecDeque;

/// A detected point on the trail. Strength includes both age and distance from the nose.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scent {
    pub at: Vector3<f32>,
    /// Seconds on the same clock used for laying and sniffing the trail.
    pub laid_at: f32,
    /// Remaining strength at the nose, from 0 to 1.
    pub strength: f32,
}

/// Sensitivity of an NPC's nose.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Nose {
    /// Maximum detection distance in meters.
    pub range: f32,
    /// Seconds for a sample's strength to halve.
    pub half_life: f32,
    /// Minimum strength at the nose, greater than 0 and at most 1.
    pub threshold: f32,
}

impl Nose {
    pub const DEFAULT: Self = Self {
        range: 2.5,
        half_life: 20.0,
        threshold: 0.1,
    };
}

impl Default for Nose {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// A bounded history of scent samples from a single source. Use a separate trail for each
/// source a dog can distinguish.
#[derive(Debug, Clone)]
pub struct Trail {
    samples: VecDeque<Scent>,
    capacity: usize,
}

impl Trail {
    /// Keep up to `capacity` samples, discarding the oldest first.
    pub fn new(capacity: usize) -> Self {
        Self {
            samples: VecDeque::new(),
            capacity,
        }
    }

    /// Lay a sample at a strictly newer time than the last sample. Strength must be in (0, 1].
    /// Returns whether the sample was stored.
    pub fn lay(&mut self, at: Vector3<f32>, now: f32, strength: f32) -> bool {
        if self.capacity == 0
            || !at.iter().all(|v| v.is_finite())
            || !now.is_finite()
            || !(strength > 0.0 && strength <= 1.0)
            || self.samples.back().is_some_and(|last| now <= last.laid_at)
        {
            return false;
        }
        if self.samples.len() == self.capacity {
            self.samples.pop_front();
        }
        self.samples.push_back(Scent {
            at,
            laid_at: now,
            strength,
        });
        true
    }

    /// Find the freshest detectable sample near `feet`. Pass the timestamp of the last sample
    /// reached as `after` to keep following the trail forward. Use `None` for initial pickup.
    /// `carries(sample, feet)` can account for walls, doors, or the game's airflow model.
    pub fn sniff(
        &self,
        nose: &Nose,
        feet: Vector3<f32>,
        now: f32,
        after: Option<f32>,
        carries: impl Fn(Vector3<f32>, Vector3<f32>) -> bool,
    ) -> Option<Scent> {
        if !now.is_finite()
            || !feet.iter().all(|v| v.is_finite())
            || after.is_some_and(|time| !time.is_finite())
            || !nose.range.is_finite()
            || nose.range <= 0.0
            || !nose.half_life.is_finite()
            || nose.half_life <= 0.0
            || !(nose.threshold > 0.0 && nose.threshold <= 1.0)
        {
            return None;
        }

        self.samples
            .iter()
            .rev()
            .take_while(|sample| after.is_none_or(|time| sample.laid_at > time))
            .find_map(|sample| {
                let age = now - sample.laid_at;
                let near = 1.0 - (sample.at - feet).norm() / nose.range;
                if age < 0.0 || near <= 0.0 {
                    return None;
                }
                let strength = sample.strength * 0.5_f32.powf(age / nose.half_life) * near;
                (strength >= nose.threshold && carries(sample.at, feet)).then_some(Scent {
                    strength,
                    ..*sample
                })
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_nose_follows_fresh_samples_without_going_back_or_crossing_a_wall() {
        let mut trail = Trail::new(3);
        let feet = Vector3::zeros();
        let first = Vector3::new(0.5, 0.0, 0.0);
        let second = Vector3::new(1.0, 0.0, 0.0);
        let blocked = Vector3::new(0.0, 0.0, 1.0);
        assert!(trail.lay(first, 1.0, 1.0));
        assert!(trail.lay(second, 2.0, 1.0));
        assert!(trail.lay(blocked, 3.0, 1.0));
        let carries = |sample: Vector3<f32>, _| sample.z == 0.0;
        let nose = Nose::default();

        assert_eq!(
            trail.sniff(&nose, feet, 1.5, None, carries).unwrap().at,
            first
        );
        assert_eq!(
            trail
                .sniff(&nose, feet, 4.0, Some(1.0), carries)
                .unwrap()
                .at,
            second
        );
        assert!(trail.sniff(&nose, feet, 4.0, Some(2.0), carries).is_none());
        assert!(trail.sniff(&nose, feet, 100.0, None, carries).is_none());
    }

    #[test]
    fn retention_and_strength_limit_detection() {
        let mut trail = Trail::new(1);
        let feet = Vector3::zeros();
        let nose = Nose {
            range: 2.0,
            half_life: 2.0,
            threshold: 0.2,
        };
        assert!(trail.lay(feet, 0.0, 1.0));
        assert!(trail.lay(Vector3::x(), 1.0, 1.0));
        assert!(!trail.lay(feet, 1.0, 1.0));
        assert!(trail.sniff(&nose, feet, 0.5, None, |_, _| true).is_none());
        let scent = trail.sniff(&nose, feet, 3.0, None, |_, _| true).unwrap();
        assert_eq!(scent.strength, 0.25);
        assert!(trail.sniff(&nose, feet, 5.0, None, |_, _| true).is_none());
    }
}
