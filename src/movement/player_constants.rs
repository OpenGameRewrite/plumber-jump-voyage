//! Per-player values selected by placement `IsMoon` in SMO 1.0.0
//! (`initPlayer` 0x41b334 -> `createMarioConst` 0x4479b8).
//! Defaults are PlayerConst constructor values; Moon values are data from
//! PlayerConstMoon. Missing keys retain the defaults, rather than scaling gravity.

/// The 44 values overridden by PlayerConstMoon. Values for movement families
/// not yet implemented stay here so their eventual consumers use the same set.
#[derive(Clone, Copy, Debug, PartialEq)]
#[allow(dead_code)]
pub struct PlayerConstants {
    /// `AnimFrameRateMaxDash`.
    pub anim_frame_rate_max_dash: f32,
    /// `AnimFrameRateMaxDashFast`.
    pub anim_frame_rate_max_dash_fast: f32,
    /// `AnimFrameRateMaxRun`.
    pub anim_frame_rate_max_run: f32,
    /// `CapCatchPopGravity`.
    pub cap_catch_pop_gravity: f32,
    /// `CapHeadSpringJumpGravity`.
    pub cap_head_spring_jump_gravity: f32,
    /// `CapHeadSpringJumpGravityHigh`.
    pub cap_head_spring_jump_gravity_high: f32,
    /// `CapLeapFrogJumpGravity`.
    pub cap_leap_frog_jump_gravity: f32,
    /// `CapLeapFrogJumpPower`.
    pub cap_leap_frog_jump_power: f32,
    /// `CapLeapFrogJumpPowerAir`.
    pub cap_leap_frog_jump_power_air: f32,
    /// `CenterTiltRateMax`.
    pub center_tilt_rate_max: f32,
    /// `GrabCeilLeavePopGravity`.
    pub grab_ceil_leave_pop_gravity: f32,
    /// `GravityAir`.
    pub gravity_air: f32,
    /// `GravityDamage`.
    pub gravity_damage: f32,
    /// `GravityWallSlide`.
    pub gravity_wall_slide: f32,
    /// `HeadSlidingGravityAir`.
    pub head_sliding_gravity_air: f32,
    /// `HeadSlidingJump`.
    pub head_sliding_jump: f32,
    /// `HeadSlidingSpeed`.
    pub head_sliding_speed: f32,
    /// `HipDropGravity`.
    pub hip_drop_gravity: f32,
    /// `HipDropSpeed`.
    pub hip_drop_speed: f32,
    /// `JumpGravity`.
    pub jump_gravity: f32,
    /// `JumpGravity2nd`.
    pub jump_gravity2nd: f32,
    /// `JumpGravity3rd`.
    pub jump_gravity3rd: f32,
    /// `JumpGravityCapCatch`.
    pub jump_gravity_cap_catch: f32,
    /// `JumpGravityForceRun`.
    pub jump_gravity_force_run: f32,
    /// `JumpHipDropPower`.
    pub jump_hip_drop_power: f32,
    /// `JumpPowerMax2DArea`.
    pub jump_power_max2d_area: f32,
    /// `JumpPowerMin2DArea`.
    pub jump_power_min2d_area: f32,
    /// `LongJumpGravity`.
    pub long_jump_gravity: f32,
    /// `SpinFlowerJumpDownFallInitSpeed`.
    pub spin_flower_jump_down_fall_init_speed: f32,
    /// `SpinFlowerJumpDownFallPower`.
    pub spin_flower_jump_down_fall_power: f32,
    /// `SpinJumpDownFallInitSpeed`.
    pub spin_jump_down_fall_init_speed: f32,
    /// `SpinJumpGravity`.
    pub spin_jump_gravity: f32,
    /// `SquatJumpGravity`.
    pub squat_jump_gravity: f32,
    /// `TrampleGravity`.
    pub trample_gravity: f32,
    /// `TrampleGravity2D`.
    pub trample_gravity2d: f32,
    /// `TrampleHighGravity`.
    pub trample_high_gravity: f32,
    /// `TrampleHighGravity2D`.
    pub trample_high_gravity2d: f32,
    /// `TrampleHighJumpPower2D`.
    pub trample_high_jump_power2d: f32,
    /// `TrampleHipDropGravity`.
    pub trample_hip_drop_gravity: f32,
    /// `TrampleHipDropJumpPower`.
    pub trample_hip_drop_jump_power: f32,
    /// `TurnJumpGravity`.
    pub turn_jump_gravity: f32,
    /// `WallClimbJumpGravity`.
    pub wall_climb_jump_gravity: f32,
    /// `WallJumpGravity`.
    pub wall_jump_gravity: f32,
    /// `WallJumpInvalidateInputFrame`.
    pub wall_jump_invalidate_input_frame: u32,
}

impl PlayerConstants {
    pub const NORMAL: Self = Self {
        anim_frame_rate_max_dash: 4.0,
        anim_frame_rate_max_dash_fast: 5.0,
        anim_frame_rate_max_run: 3.5,
        cap_catch_pop_gravity: 0.8,
        cap_head_spring_jump_gravity: 1.2,
        cap_head_spring_jump_gravity_high: 1.0,
        cap_leap_frog_jump_gravity: 1.0,
        cap_leap_frog_jump_power: 32.0,
        cap_leap_frog_jump_power_air: 26.0,
        center_tilt_rate_max: 0.0,
        grab_ceil_leave_pop_gravity: 1.2,
        gravity_air: 1.5,
        gravity_damage: 0.95,
        gravity_wall_slide: 0.5,
        head_sliding_gravity_air: 2.0,
        head_sliding_jump: 28.0,
        head_sliding_speed: 20.0,
        hip_drop_gravity: 45.0,
        hip_drop_speed: 45.0,
        jump_gravity: 1.5,
        jump_gravity2nd: 1.5,
        jump_gravity3rd: 1.0,
        jump_gravity_cap_catch: 1.3,
        jump_gravity_force_run: 1.0,
        jump_hip_drop_power: 40.0,
        jump_power_max2d_area: 27.5,
        jump_power_min2d_area: 23.5,
        long_jump_gravity: 0.48,
        spin_flower_jump_down_fall_init_speed: 15.0,
        spin_flower_jump_down_fall_power: 2.0,
        spin_jump_down_fall_init_speed: 35.0,
        spin_jump_gravity: 0.4,
        squat_jump_gravity: 1.0,
        trample_gravity: 1.75,
        trample_gravity2d: 1.75,
        trample_high_gravity: 1.0,
        trample_high_gravity2d: 1.0,
        trample_high_jump_power2d: 32.0,
        trample_hip_drop_gravity: 1.5,
        trample_hip_drop_jump_power: 35.0,
        turn_jump_gravity: 1.0,
        wall_climb_jump_gravity: 1.5,
        wall_jump_gravity: 0.95,
        wall_jump_invalidate_input_frame: 25,
    };
    pub const MOON: Self = Self {
        anim_frame_rate_max_dash: 3.5,
        anim_frame_rate_max_dash_fast: 4.5,
        anim_frame_rate_max_run: 3.0,
        cap_catch_pop_gravity: 0.6,
        cap_head_spring_jump_gravity: 0.8,
        cap_head_spring_jump_gravity_high: 0.6,
        cap_leap_frog_jump_gravity: 0.4,
        cap_leap_frog_jump_power: 30.0,
        cap_leap_frog_jump_power_air: 25.0,
        center_tilt_rate_max: 1.0,
        grab_ceil_leave_pop_gravity: 0.6,
        gravity_air: 1.0,
        gravity_damage: 0.7,
        gravity_wall_slide: 0.2,
        head_sliding_gravity_air: 0.8,
        head_sliding_jump: 17.0,
        head_sliding_speed: 18.0,
        hip_drop_gravity: 1.5,
        hip_drop_speed: 0.0,
        jump_gravity: 0.4,
        jump_gravity2nd: 0.4,
        jump_gravity3rd: 0.3,
        jump_gravity_cap_catch: 0.6,
        jump_gravity_force_run: 0.35,
        jump_hip_drop_power: 32.0,
        jump_power_max2d_area: 24.0,
        jump_power_min2d_area: 20.5,
        long_jump_gravity: 0.2,
        spin_flower_jump_down_fall_init_speed: 0.0,
        spin_flower_jump_down_fall_power: 1.5,
        spin_jump_down_fall_init_speed: 0.0,
        spin_jump_gravity: 0.18,
        squat_jump_gravity: 0.45,
        trample_gravity: 0.5,
        trample_gravity2d: 0.5,
        trample_high_gravity: 0.4,
        trample_high_gravity2d: 0.4,
        trample_high_jump_power2d: 27.0,
        trample_hip_drop_gravity: 0.4,
        trample_hip_drop_jump_power: 30.0,
        turn_jump_gravity: 0.45,
        wall_climb_jump_gravity: 0.8,
        wall_jump_gravity: 0.3,
        wall_jump_invalidate_input_frame: 30,
    };
}
