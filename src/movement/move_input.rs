//! The stick laid on the world (`PlayerInput::calcMoveInput`, the ordinary 3D path): the camera's
//! view matrix and the surface's up decide the two axes the stick's x and y push along, the push
//! is cut to the stick's own length and then bent along a wall (`wall_along.rs`).
//!
//! Single precision in the executable's operation order (`calcMoveInputImpl` 0x44b44c, the axes
//! 0x44f864), checked bit for bit against the executable's code run in a local interpreter
//! (`PJV_MOVE_INPUT_CASES`, see `tests/move_input.rs`). Not ported: the 2D areas, the held-3D
//! input, the area snap and the separate play ports.
use super::pose_math::{
    NEGATED_GRAVITY, along_vector_normal_h, cross, dot, normalize, squared_length,
    try_normalize_or_zero,
};
use super::vector_lib::verticalize_vec;
use bevy::math::{Vec2, Vec3};

/// The rotation part of the view matrix (`PlayerFunction::getPlayerViewMtx`, a `Matrix34f`,
/// world to view) by rows: the camera's side axis, its up axis and its back axis.
pub type ViewRows = [Vec3; 3];

/// The stick is no input under this length.
const STICK_DEADZONE: f32 = 0.1;

/// The view matrix the probe pins the camera to (`setLockedView`): the camera looks along
/// (sin, 0, cos) of its yaw, which the scenario file carries as the single precision values of
/// the double precision sine and cosine of the angle in radians.
pub fn locked_view(yaw_deg: f32) -> ViewRows {
    let radians = f64::from(yaw_deg).to_radians();
    let (forward_x, forward_z) = (radians.sin() as f32, radians.cos() as f32);
    [
        Vec3::new(-forward_z, 0., forward_x),
        Vec3::new(0., 1., 0.),
        Vec3::new(-forward_x, 0., -forward_z),
    ]
}

/// The two axes the stick pushes along (0x44f864): `(side, forward)`. `forward` is the camera's
/// view direction, or the camera's up (down) axis when the camera looks along the gravity;
/// both are then laid in the plane of `up`, the forward axis carried from the plane across the
/// camera's side axis to the plane the gravity-up makes with it.
pub fn input_axes(view: &ViewRows, gravity_up: Vec3, up: Vec3) -> (Vec3, Vec3) {
    let [side_row, up_row, back_row] = *view;
    let toward_up = dot(up_row, gravity_up);
    // `-(back . gravity_up)`, written as the executable's subtractions.
    let looking =
        ((gravity_up.x * -back_row.x) - (gravity_up.y * back_row.y)) - (gravity_up.z * back_row.z);
    let forward = if looking.abs() > toward_up.abs() {
        if looking >= 0. { -up_row } else { up_row }
    } else if toward_up < 0. {
        back_row
    } else {
        Vec3::new(-back_row.x, -back_row.y, -back_row.z)
    };
    let across = normalize(cross(side_row, forward));
    let (kept, usable) = try_normalize_or_zero(verticalize_vec(side_row, up));
    let target = if usable { kept } else { up };
    let forward = normalize(along_vector_normal_h(forward, across, target));
    let side = normalize(cross(forward, up));
    (side, forward)
}

/// `calcMoveInput(out, up)` before the wall bend: the stick (+y up) pushed along the axes,
/// the push cut to the stick's length (at most 1), nothing under the dead zone.
pub fn move_input(stick: Vec2, view: &ViewRows, up: Vec3) -> Vec3 {
    let (side, forward) = input_axes(view, NEGATED_GRAVITY, up);
    let push = Vec3::new(
        stick.x * side.x + stick.y * forward.x,
        stick.x * side.y + stick.y * forward.y,
        stick.x * side.z + stick.y * forward.z,
    );
    let tilt = ((stick.x * stick.x) + (stick.y * stick.y)).sqrt().min(1.);
    if tilt < STICK_DEADZONE {
        return Vec3::ZERO;
    }
    let length = squared_length(push).sqrt();
    if length <= 0. {
        return push;
    }
    let scale = tilt / length;
    Vec3::new(scale * push.x, scale * push.y, scale * push.z)
}

#[path = "../tests/move_input.rs"]
mod checks;

pub fn self_check() {
    checks::run();
}
