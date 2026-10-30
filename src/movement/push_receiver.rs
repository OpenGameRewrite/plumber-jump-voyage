//! Sensor separation, independently of locomotion and WallPush.
//! SMO 1.0.0: calcPushTrans 0x59bdf4, PlayerPushReceiver 0x45fc90..0x460334.
use super::pose_math::{cross, dot, squared_length, try_normalize_or_zero};
use super::vector_lib::{length, verticalize_vec};
use bevy::prelude::Vec3;

#[derive(Clone, Debug, Default)]
pub struct PushReceiver {
    pub min: Vec3,
    pub max: Vec3,
    pub collide_min: Vec3,
    pub collide_max: Vec3,
}

pub fn calc_push_trans(
    receiver: Vec3,
    receiver_radius: f32,
    sender: Vec3,
    sender_radius: f32,
    gravity: Vec3,
    front: Vec3,
    max_trans: f32,
) -> Vec3 {
    let radius = receiver_radius + sender_radius;
    let difference = receiver - sender;
    if squared_length(difference) > radius * radius {
        return Vec3::ZERO;
    }
    let (direction, valid) = try_normalize_or_zero(difference);
    let vertical_rate = if valid {
        length(cross(direction, gravity)).clamp(0.1, 1.)
    } else {
        1.
    };
    let horizontal = verticalize_vec(gravity, difference);
    let power = (radius * vertical_rate - length(horizontal))
        .max(0.)
        .min(max_trans);
    let (direction, valid) = try_normalize_or_zero(horizontal);
    (if valid { direction } else { -front }) * power
}

impl PushReceiver {
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn receive(&mut self, force: Vec3, collide: bool) {
        self.min = self.min.min(force);
        self.max = self.max.max(force);
        if collide {
            self.collide_min = self.collide_min.min(force);
            self.collide_max = self.collide_max.max(force);
        }
    }

    pub fn pushed_velocity(&self, velocity: Vec3, wall: Option<(Vec3, f32)>) -> Vec3 {
        let mut push = self.min + self.max;
        if let Some((normal, penetration)) = wall
            && penetration >= 5.
        {
            let along_push = dot(normal, push);
            let along_velocity = dot(normal, velocity);
            let sign = |v: f32| if v < 0. { -1. } else { 1. };
            let moving_in = (sign(along_velocity * along_push) * along_velocity.abs()).max(0.);
            let limit = (35. - penetration - moving_in).max(0.);
            let force = along_push.abs();
            if force > limit {
                push += normal * (-(force - limit).max(0.) * sign(along_push));
            }
        }
        pushed_velocity_common(velocity, push)
    }
}

pub fn pushed_velocity_common(velocity: Vec3, push: Vec3) -> Vec3 {
    let (direction, valid) = try_normalize_or_zero(push);
    if !valid {
        return velocity;
    }
    let force = length(push);
    let along_velocity = dot(direction, velocity);
    let (velocity_direction, moving) = try_normalize_or_zero(velocity);
    let offset = if moving {
        let rate = (1. - dot(direction, velocity_direction)).clamp(0., 1.);
        // sead clamp uses this order even when the upper bound is below 1.
        if rate * along_velocity > -1. {
            1.
        } else {
            (-rate * along_velocity).min(force * 0.9)
        }
    } else {
        1.
    };
    let amount = along_velocity.max((force - offset).max(0.));
    verticalize_vec(direction, velocity) + direction * amount
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contacts_use_extrema_and_preserve_transverse_velocity() {
        let mut receiver = PushReceiver::default();
        receiver.receive(Vec3::new(-4., 0., 2.), false);
        receiver.receive(Vec3::new(-2., 0., 3.), true);
        receiver.receive(Vec3::new(1., 0., 0.), false);
        assert_eq!(receiver.min + receiver.max, Vec3::new(-3., 0., 3.));
        assert_eq!(
            receiver.collide_min + receiver.collide_max,
            Vec3::new(-2., 0., 3.)
        );
        assert_eq!(
            pushed_velocity_common(Vec3::new(2., -3., 4.), Vec3::new(0., 0., -10.)),
            Vec3::new(2., -3., -6.)
        );
        receiver.clear();
        assert_eq!(receiver.pushed_velocity(Vec3::ONE, None), Vec3::ONE);
    }
}
