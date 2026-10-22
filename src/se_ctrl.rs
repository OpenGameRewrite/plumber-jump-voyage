//! The sounds the original's player code starts by itself each frame, apart
//! from the ones its sound tables attach to animation frames and reactions.
//!
//! Read from the 1.0.0 executable's per-frame sound control and confirmed
//! against recorded runs (every footfall of the walk and run scenarios):
//!
//! - **Footsteps** are not tied to clip frames. While the `Move` slot plays, a
//!   counter of frames moved since the last footfall decides: a foot-lift sound
//!   (`RunUpL`/`RunUpR`) a bit before the next footfall, then the footfall
//!   itself (`RunL`/`RunR` with its material variant and cloth rustle). The
//!   gap is a number of animation frames (30, shorter at a fast pace, shorter
//!   while the run start blends in), divided by the animation rate. A footfall
//!   that lands far from the animation's own beat (0 or 30 of the 60-frame
//!   cycle) pulls the next one half that far toward it.
//! - **Rolling** starts `Roll` each time the rolling clip passes its sixth frame.
//! - **Falling fast** holds `FallWhoosh_loop` every frame the vertical speed is
//!   30 units a frame down or more, counting the frames it has held.
//! - **The water around the camera**: a sound when the camera dips into the water or comes
//!   out of it, and an ambience held every frame the camera and the player are both under.
//! - **The swim stroke** (`SwimStand`) sounds when its clip begins or loops (its frame went
//!   back since the last update), the bubbles louder from under the water.
//!
//! The control lives in the player so one source feeds the sound, the dust and
//! the headless trace.

/// Fraction of the footfall gap after which the foot-lift sound plays.
const FOOT_LIFT_AT: f32 = 0.69;
/// The animation's own footfall beat: every half cycle of its 60 frames.
const CYCLE_HALF: f32 = 30.;
/// A pace at which the gap stops shrinking, and its shortest gap, in animation frames.
const FAST_GAP: f32 = 22.;
/// Corrections of a footfall's place in the cycle only apply below this rate.
const CORRECTED_BELOW_RATE: f32 = 2.;
/// Footfalls this many animation frames off the beat are left alone.
const BEAT_TOLERANCE: f32 = 2.;
/// A rate or a speed this small counts as standing still.
const NEAR_ZERO: f32 = 0.001;
/// Downward speed in units a frame from which the fall is loud.
const WHOOSH_SPEED: f32 = 30.;
/// Frame of the rolling clip after which its roll sound starts.
const ROLL_SOUND_FRAME: f32 = 6.;
/// A new player has walked for a full gap already: the first run frame lifts a foot.
const FIRST_COUNT: i32 = 30;

/// What the player did this frame, as far as these sounds care.
#[derive(Clone, Copy, Debug, Default)]
pub struct SeFrame {
    /// The run cycle plays and no whole-body clip covers it.
    pub stepping: bool,
    /// The animator's rate and counter of the run cycle.
    pub rate: f32,
    pub phase: f32,
    /// Weight of the run-start clip in the pose.
    pub run_start: f32,
    /// Horizontal speed in units a frame.
    pub speed: f32,
    /// Vertical speed in units a frame (negative falling).
    pub vertical_speed: f32,
    /// The player is in a water area (`al::isInWater`), where the fall is silent.
    pub in_water: bool,
    /// Frame of the rolling clip, while it plays.
    pub rolling: Option<f32>,
    /// The frame the animator shows now (`PlayerAnimator::getAnimFrame`).
    pub anim_frame: f32,
    /// Frame of the swim stroke's clip, while it plays.
    pub stroke: Option<f32>,
    /// Where the camera is, when it is known.
    pub camera: Option<CameraWater>,
}

/// The camera's relation to the water.
#[derive(Clone, Copy, Debug, Default)]
pub struct CameraWater {
    /// The camera is in a water area, ignored areas too (`al::isInWaterNoIgnore`).
    pub under: bool,
    /// The camera and the player are both in water (`al::isInWaterPos` and `al::isInWater`).
    pub ambience: bool,
}

/// A sound request of the control.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SeEvent {
    /// A foot lifts for the next step (`RunUp` + side).
    FootLift { left: bool },
    /// A foot comes down (`Run`, `RunMat`, `ShakeNoise` + side).
    Footfall { left: bool },
    /// The roll's spin sound.
    Roll,
    /// The fall's wind, held: the consecutive frames it has held.
    FallWhoosh { frames: u32 },
    /// The camera dipped into the water or came out of it.
    CameraWater { entered: bool },
    /// The camera's underwater ambience, held.
    CameraWaterHold,
    /// The swim stroke's bubbles; louder when the camera is under the water.
    SwimStroke { camera_under: bool },
}

#[derive(Clone, Debug)]
pub struct SeCtrl {
    /// Frames moved since the last footfall (the count keeps running between runs).
    moved: i32,
    /// The next footfall is the left foot's.
    left: bool,
    /// The foot-lift sound of this gap has played.
    lifted: bool,
    /// Frame of the rolling clip last seen.
    roll_frame: f32,
    /// Frames the loud fall has held; -1 until the first quiet frame.
    falling: i32,
    /// The camera was under the water in the last update (`this+0x168`).
    camera_under: bool,
    /// The animator's frame in the last update (`this+0x18`).
    last_anim_frame: f32,
}
impl Default for SeCtrl {
    fn default() -> Self {
        Self {
            moved: FIRST_COUNT,
            left: false,
            lifted: false,
            roll_frame: 0.,
            falling: -1,
            camera_under: false,
            last_anim_frame: 0.,
        }
    }
}

impl SeCtrl {
    /// One frame, after the player moved and animated.
    pub fn update(&mut self, frame: &SeFrame) -> Vec<SeEvent> {
        let mut events = Vec::new();
        self.roll(frame, &mut events);
        if frame.stepping {
            self.footsteps(frame, &mut events);
        }
        self.whoosh(frame, &mut events);
        self.water(frame, &mut events);
        self.last_anim_frame = frame.anim_frame;
        // Moving counts toward the next footfall in every state.
        if frame.speed.abs() >= NEAR_ZERO {
            self.moved += 1;
        }
        events
    }

    /// Animation frames between footfalls at this pace.
    fn gap(frame: &SeFrame) -> f32 {
        if frame.run_start > 0. {
            // The run start's steps come quickly and slow as it blends out.
            return CYCLE_HALF - 18. * frame.run_start;
        }
        (46. - 8. * frame.rate).clamp(FAST_GAP, CYCLE_HALF)
    }

    fn footsteps(&mut self, frame: &SeFrame, events: &mut Vec<SeEvent>) {
        if frame.rate.abs() < NEAR_ZERO {
            return;
        }
        let gap = Self::gap(frame);
        let moved = self.moved as f32;
        if !self.lifted && moved >= gap * FOOT_LIFT_AT / frame.rate {
            self.lifted = true;
            events.push(SeEvent::FootLift { left: self.left });
            return;
        }
        if moved < gap / frame.rate {
            return;
        }
        self.moved = 0;
        if frame.rate < CORRECTED_BELOW_RATE {
            let off = beat_offset(frame.phase);
            if (off.abs() as f32) > BEAT_TOLERANCE / frame.rate {
                self.moved += (off as f32 * 0.5) as i32;
            }
        }
        self.left = !self.left;
        self.lifted = false;
        events.push(SeEvent::Footfall { left: self.left });
    }

    fn roll(&mut self, frame: &SeFrame, events: &mut Vec<SeEvent>) {
        let Some(now) = frame.rolling else {
            self.roll_frame = 0.;
            return;
        };
        if self.roll_frame > now {
            self.roll_frame = 0.;
        }
        if now > ROLL_SOUND_FRAME && self.roll_frame <= ROLL_SOUND_FRAME {
            events.push(SeEvent::Roll);
        }
        self.roll_frame = now;
    }

    fn water(&mut self, frame: &SeFrame, events: &mut Vec<SeEvent>) {
        let under = frame.camera.is_some_and(|camera| camera.under);
        if let Some(camera) = frame.camera {
            if camera.under != self.camera_under {
                events.push(SeEvent::CameraWater {
                    entered: camera.under,
                });
            }
            if camera.ambience {
                events.push(SeEvent::CameraWaterHold);
            }
        }
        self.camera_under = under;
        // The stroke's sounds start when the clip's frame went back (or it just began).
        if let Some(now) = frame.stroke
            && self.last_anim_frame > now
        {
            events.push(SeEvent::SwimStroke {
                camera_under: under,
            });
        }
    }

    fn whoosh(&mut self, frame: &SeFrame, events: &mut Vec<SeEvent>) {
        if frame.in_water || frame.vertical_speed > -WHOOSH_SPEED {
            self.falling = 0;
            return;
        }
        let before = self.falling;
        self.falling += 1;
        if before >= 0 {
            events.push(SeEvent::FallWhoosh {
                frames: self.falling as u32,
            });
        }
    }
}

/// Animation frames (whole, toward zero) the cycle is past its nearest beat:
/// negative before it. The beats are the cycle's start and its half.
fn beat_offset(phase: f32) -> i32 {
    if phase < 15. {
        phase as i32
    } else if phase >= 45. {
        -((60. - phase) as i32)
    } else {
        (phase - 30.) as i32
    }
}

#[path = "tests/se_ctrl.rs"]
mod checks;

pub fn self_check() {
    checks::run();
}
