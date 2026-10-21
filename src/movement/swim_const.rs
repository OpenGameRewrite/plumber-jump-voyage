//! The swim values of `PlayerConst`, in centimetres and frames. Mario's constants come from the
//! built-in defaults (`PlayerConst::PlayerConst()`); the byml that could override them exists
//! only for the moon-gravity variant.
//!
//! Each constant is named after its getter (`getSwimSurfaceStartDist` is `SURFACE_START_DIST`).

/// The surface is "near" when it is closer than this above the root (`SwimSurfaceStartDist`).
pub const SURFACE_START_DIST: f32 = 120.;
/// The surface nerve lets go when the surface is this far (`SwimSurfaceEndDist`).
pub const SURFACE_END_DIST: f32 = 200.;
/// The root's offset from the water line that the player's centre is judged at
/// (`SwimCenterOffset`): the in-water judge also tests the point this far above the feet.
pub const CENTER_OFFSET: f32 = 80.;
/// The spring of the surface nerve (`SwimSurfaceBaseHeight`, `SwimSurfaceSpring`,
/// `SwimSurfaceDamper`): the root settles this far below the surface when idle.
pub const SURFACE_BASE_HEIGHT: f32 = 80.;
pub const SURFACE_SPRING: f32 = 0.05;
pub const SURFACE_DAMPER: f32 = 0.949;
/// The same while swimming on (`SwimSurfaceMove*`).
pub const SURFACE_MOVE_BASE_HEIGHT: f32 = 80.;
pub const SURFACE_MOVE_SPRING: f32 = 0.01;
pub const SURFACE_MOVE_DAMPER: f32 = 0.94;
/// The damper counter ramps over `SURFACE_DAMPER_FRAME` steps once it is past
/// `SURFACE_DAMPER_START` (`SwimSurfaceDamperStart`, `SwimSurfaceDamperFrame`).
pub const SURFACE_DAMPER_START: i32 = 25;
pub const SURFACE_DAMPER_FRAME: i32 = 25;
/// `SwimSurfaceGravity`: the pull on a body at the surface.
pub const SURFACE_GRAVITY: f32 = 0.125;
/// `SwimSurfaceAccelH`, `SwimSurfaceSpeedMaxH`.
pub const SURFACE_ACCEL_H: f32 = 0.25;
pub const SURFACE_SPEED_MAX_H: f32 = 9.;
/// `SwimSurfaceSpinCapSpeedMaxH`.
pub const SURFACE_SPIN_CAP_SPEED_MAX_H: f32 = 13.;
/// A jump from the surface is allowed while the surface is within this (`SwimSurfaceEnableJumpHeight`)
/// and pre-inputs the button for `SURFACE_PRE_INPUT_JUMP_FRAME` steps.
pub const SURFACE_ENABLE_JUMP_HEIGHT: f32 = 160.;
pub const SURFACE_PRE_INPUT_JUMP_FRAME: i32 = 8;

/// `SwimGravity`, `SwimGravityWalk`, `SwimFallSpeedMax`.
pub const GRAVITY: f32 = 0.25;
pub const GRAVITY_WALK: f32 = 1.;
pub const FALL_SPEED_MAX: f32 = 6.5;
/// `SwimRisePower`, `SwimRiseFrame`, `SwimRiseSpeedMax`: the paddle's rise.
pub const RISE_POWER: f32 = 0.6;
pub const RISE_FRAME: i32 = 10;
pub const RISE_SPEED_MAX: f32 = 7.5;

/// A jump from the surface is refused this many steps after falling in (`SwimFallInvalidJumpFrame`).
pub const FALL_IN_INVALID_JUMP_FRAME: i32 = 8;

/// `SwimBrakeRateH`.
pub const BRAKE_RATE_H: f32 = 0.975;
/// `SwimFloorAccelH`, `SwimFloorSpeedMaxH`: on the bottom.
pub const FLOOR_ACCEL_H: f32 = 0.125;
pub const FLOOR_SPEED_MAX_H: f32 = 6.5;
/// `SwimHighAccelH`, `SwimHighSpeedMaxH`, the faster stroke allowed for
/// `HIGH_ACCEL_PERMIT_FRAME` steps after a paddle.
pub const HIGH_ACCEL_H: f32 = 0.25;
pub const HIGH_SPEED_MAX_H: f32 = 7.5;
pub const HIGH_ACCEL_PERMIT_FRAME: i32 = 35;
/// `SwimLowAccelH`, `SwimLowSpeedMaxH`.
pub const LOW_ACCEL_H: f32 = 0.25;
pub const LOW_SPEED_MAX_H: f32 = 6.5;
/// `SwimWallHitSpeedMinH`.
pub const WALL_HIT_SPEED_MIN_H: f32 = 3.;

/// The swim turn (`SwimRot*`): the speed that changes the turn rate, the turn speed forward and
/// at most, the controller's angles and ramps.
pub const ROT_SPEED_CHANGE_START: f32 = 3.;
pub const ROT_SPEED_FORWARD: f32 = 3.;
pub const ROT_SPEED_MAX: f32 = 7.5;
pub const ROT_START_ANGLE: f32 = 1.;
pub const ROT_FAST_ANGLE: f32 = 80.;
pub const ROT_ACCEL_FRAME: i32 = 20;
pub const ROT_ACCEL_FRAME_FAST: i32 = 1;
pub const ROT_BRAKE_FRAME: i32 = 30;
