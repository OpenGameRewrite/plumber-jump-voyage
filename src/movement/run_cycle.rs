//! The run cycle's animation control: which clips of the `Move` slot play, at
//! what weights and speed.
//!
//! Written from the original's `PlayerAnimControlRun::update` and
//! `PlayerAnimFrameCtrl`, as read from the 1.0.0 executable and confirmed
//! against recorded runs. The slot holds Walk, Run, Dash, DashFast and
//! RunStart; all of them play one 60-frame phase, each clip at its own length.
use super::ground::rate01;
use super::t;

/// The `Move` slot's clips with their lengths in frames; the phase is spread
/// over each (`frame = phase * length / 60`).
pub const LAYERS: [(&str, f32); 5] = [
    ("Walk", 60.),
    ("Run", 17.),
    ("Dash", 17.),
    ("DashFast", 14.),
    ("RunStart", 30.),
];
const CYCLE: f32 = 60.;
/// Speeds that mark the blends: Walk to Run around the run border, Run to Dash
/// around the dash border, Dash to DashFast around the dash-fast border.
const RUN_BORDER: f32 = 8.;
const RUN_RANGE: f32 = 2.;
const DASH_BORDER: f32 = 15.;
const DASH_RANGE: f32 = 1.;
const DASH_FAST_BORDER: f32 = 20.;
const DASH_FAST_RANGE: f32 = 1.;
/// Playback rates of the cycle: the walk's pace goes with the slowest speed
/// and the dash-fast's with the speed where the animation stops speeding up.
const RATE_SPEED_MIN: f32 = 6.;
const RATE_SPEED_MAX: f32 = 26.;
const RATE_RUN_START: f32 = 2.;
const RATE_MIN_RUN: f32 = 1.;
const RATE_MAX_RUN: f32 = 3.5;
const RATE_MAX_DASH: f32 = 4.;
const RATE_MAX_DASH_FAST: f32 = 5.;
/// Frames the run-start clip fades in and out over.
const START_BLEND_FRAMES: i32 = 5;
/// A stick pushed this far counts as full tilt.
const FULL_TILT: f32 = 0.95;

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    (1. - t) * a + t * b
}
fn converge(value: i32, target: i32) -> i32 {
    if value < target {
        value + 1
    } else {
        (value - 1).max(target)
    }
}
/// A blend that rises from 0 to 1 across `range` either side of `border`.
fn ramp(speed: f32, border: f32, range: f32) -> f32 {
    if range.abs() < 0.001 {
        return 0.;
    }
    ((range + speed - border) / (range + range)).clamp(0., 1.)
}

/// One layer of the pose: a clip of the slot with its frame in that clip.
pub struct Layer {
    pub name: &'static str,
    pub weight: f32,
    pub frame: f32,
    pub rate: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RunCycle {
    /// The run-start clip may blend in (a run that begins from a standstill).
    start_allowed: bool,
    start_frames: i32,
    blend_frames: i32,
    /// Weights of Walk, Run, Dash, DashFast and RunStart.
    pub weights: [f32; 5],
    /// Playback rate in cycle frames per game frame.
    pub rate: f32,
    /// The pose's place in the cycle after the latest frame.
    pub phase: f32,
    /// The frame counter the animator reports at the start of a frame: the
    /// previous pose plus the previous rate (not wrapped).
    pub shown: f32,
    /// Frame of the run-start clip, which plays once and holds its last frame.
    start_clip: f32,
    /// This frame's advance of the phase is done.
    advanced: bool,
}
impl RunCycle {
    /// The cycle starts playing: the first frame shows its first pose.
    pub fn start(&mut self) {
        self.phase = 0.;
        self.shown = 0.;
        self.rate = 1.;
        self.advanced = true;
        self.start_clip = 0.;
        self.weights = [0.; 5];
    }
    /// The pose moves on by the current rate.
    fn move_on(&mut self) {
        self.phase = (self.phase + self.rate) % CYCLE;
        let start_length = LAYERS[4].1;
        self.start_clip = (self.start_clip + self.rate * start_length / CYCLE).min(start_length);
    }
    /// At the start of a game frame the animator's counter reads the previous
    /// pose moved on by the previous rate.
    pub fn begin_frame(&mut self) {
        if !self.advanced {
            // The last frame did not run the control: the pose moved on anyway.
            self.move_on();
        }
        self.shown = self.phase + self.rate;
        self.advanced = false;
    }
    /// `PlayerAnimControlRun::reset`, in the first step of a run state.
    pub fn reset(&mut self, speed: f32, after_turn: bool) {
        self.start_allowed = !after_turn && t::NORMAL_MAX * 0.9 > speed;
        self.start_frames = 0;
        self.blend_frames = 0;
    }
    /// `PlayerAnimControlRun::update`: weights and rate for this frame's
    /// speed and stick tilt, and the pose's move on.
    pub fn update(&mut self, speed: f32, tilt: f32) {
        let fresh = self.advanced;
        let start_length = super::clip::RUN_START as i32;
        // The run-start clip fades in over a run from a standstill, plays on
        // while it lasts and fades out again.
        if tilt >= FULL_TILT && self.start_allowed && t::DASH_JUDGE > speed {
            self.blend_frames = if self.start_frames < start_length {
                converge(self.blend_frames, START_BLEND_FRAMES)
            } else {
                converge(self.blend_frames, 0)
            };
        } else {
            self.blend_frames = converge(self.blend_frames, 0);
        }
        self.start_frames = (self.start_frames + 1).min(start_length);
        let start = rate01(self.blend_frames as f32, 0., START_BLEND_FRAMES as f32);
        let keep = 1. - start;
        let (weights, rate);
        if speed > DASH_FAST_BORDER - DASH_FAST_RANGE {
            let fast = ramp(speed, DASH_FAST_BORDER, DASH_FAST_RANGE);
            weights = [0., 0., keep * (1. - fast), keep * fast, start];
            rate = lerp(
                RATE_MAX_DASH,
                RATE_MAX_DASH_FAST,
                rate01(speed, DASH_FAST_BORDER - DASH_FAST_RANGE, RATE_SPEED_MAX),
            );
        } else if tilt >= FULL_TILT || speed > DASH_BORDER - DASH_RANGE {
            let dash = ramp(speed, DASH_BORDER, DASH_RANGE);
            weights = [0., keep * (1. - dash), keep * dash, 0., start];
            rate = lerp(
                RATE_MAX_RUN,
                RATE_MAX_DASH,
                rate01(
                    speed,
                    DASH_BORDER - DASH_RANGE,
                    DASH_FAST_BORDER - DASH_FAST_RANGE,
                ),
            );
        } else if speed <= RUN_BORDER - RUN_RANGE {
            weights = [keep, 0., 0., 0., start];
            // The original ranges this over a span of no width, so the walk
            // plays at the slowest run rate.
            rate = RATE_MIN_RUN;
        } else {
            let run = ramp(speed, RUN_BORDER, RUN_RANGE);
            weights = [keep * (1. - run), keep * run, 0., 0., start];
            rate = lerp(
                RATE_MIN_RUN,
                RATE_MAX_RUN,
                rate01(speed, RUN_BORDER - RUN_RANGE, DASH_BORDER - DASH_RANGE),
            );
        }
        self.weights = weights;
        self.rate = lerp(rate, RATE_RUN_START, start);
        // The clip's first frame stays put; after that the pose moves on by
        // this frame's rate.
        if !fresh {
            self.move_on();
        }
        self.advanced = true;
        debug_assert!(RATE_SPEED_MIN < RATE_SPEED_MAX);
    }
    /// The clips that make up the pose, with their frames in their own length.
    pub fn layers(&self) -> impl Iterator<Item = Layer> + '_ {
        LAYERS
            .iter()
            .zip(self.weights)
            .filter(|(_, weight)| *weight > 0.001)
            .map(|((name, length), weight)| Layer {
                name: *name,
                weight,
                frame: if *name == "RunStart" {
                    self.start_clip
                } else {
                    self.phase * length / CYCLE
                },
                rate: self.rate * length / CYCLE,
            })
    }
    /// The state a recorded pose stands for, for lockstep stepping.
    pub fn from_recorded(phase: f32, rate: f32, weights: [f32; 5], steps: u32) -> Self {
        let length = super::clip::RUN_START as i32;
        Self {
            start_allowed: weights[4] > 0.,
            start_frames: (steps as i32).min(length),
            blend_frames: (weights[4] * START_BLEND_FRAMES as f32).round() as i32,
            weights,
            rate,
            phase,
            shown: phase + rate,
            start_clip: (steps.saturating_sub(1) as f32).min(LAYERS[4].1),
            advanced: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_stick_starts_with_the_run_start_clip_blended_in() {
        let mut cycle = RunCycle::default();
        cycle.start();
        cycle.reset(0.18, false);
        cycle.update(0.18375, 1.);
        assert!((cycle.weights[4] - 0.2).abs() < 1.0e-6);
        assert!((cycle.weights[1] - 0.8).abs() < 1.0e-6, "run, not walk");
        assert!((cycle.rate - 3.2).abs() < 1.0e-5, "{}", cycle.rate);
        assert_eq!(cycle.phase, 0.);
    }

    #[test]
    fn a_half_stick_blends_walk_and_run() {
        let mut cycle = RunCycle::default();
        cycle.start();
        cycle.reset(8.5, true);
        cycle.begin_frame();
        cycle.update(8.5, 0.5);
        assert!((cycle.weights[0] - 0.375).abs() < 1.0e-6);
        assert!((cycle.weights[1] - 0.625).abs() < 1.0e-6);
        assert!((cycle.rate - 1.78125).abs() < 1.0e-5);
    }
}
