//! The additive `cap_source` trace column: the lifecycle of every exported actor that answers the
//! cap's messages, one `id/...` record per source. Only the validators of
//! `tools/cap_sources.py` read it; no replay input depends on it.
use super::*;

impl Player {
    pub fn cap_source_diagnostic(&self) -> String {
        self.signboards
            .iter()
            .map(sign_board::SignBoard::diagnostic)
            .chain(
                self.cap_hangers
                    .iter()
                    .map(cap_hanger::CapHanger::diagnostic),
            )
            .chain(
                self.cap_switches
                    .iter()
                    .map(cap_switch::CapSwitch::diagnostic),
            )
            .collect::<Vec<_>>()
            .join("|")
    }

    /// What the cap sees of every exported lock-on target: the hangers, then the switches (the
    /// cap's event index counts them in that order).
    pub(super) fn lock_targets(&self) -> Vec<crate::cap::LockTarget> {
        let hangers = self
            .cap_hangers
            .iter()
            .map(|hanger| crate::cap::LockTarget {
                target: hanger.relative,
                body: hanger.relative + Vec3::Y * hanger.body_offset,
                body_radius: hanger.body_radius,
                waiting: hanger.waiting(),
                keeps: hanger.keeps(),
                rotation: None,
                start_action_frames: Some(cap_hanger::START_ACTION_FRAMES),
            });
        let switches = self
            .cap_switches
            .iter()
            .map(|switch| crate::cap::LockTarget {
                target: switch.target(),
                body: switch.relative + Vec3::Y * switch.body_offset,
                body_radius: switch.body_radius,
                waiting: switch.waiting(),
                keeps: switch.keeps(),
                rotation: Some(switch.target_rotation()),
                start_action_frames: None,
            });
        hangers.chain(switches).collect()
    }

    /// The CapHit sensor of every exported trampoline, in the cap's event order.
    pub(super) fn cap_hit_targets(&self) -> Vec<crate::cap::CapHitTarget> {
        self.trampolines
            .iter()
            .map(cap_trampoline::Trampoline::cap_hit_target)
            .collect()
    }

    /// One execution of every lock-on target's nerve; a switch's activation stops the scene.
    pub(super) fn tick_lock_on_sources(&mut self, keep_input: bool) {
        let player = self.position_cm();
        for hanger in &mut self.cap_hangers {
            let distance =
                Vec2::new(player.x - hanger.relative.x, player.z - hanger.relative.z).length();
            hanger.tick(keep_input, distance);
        }
        for switch in &mut self.cap_switches {
            if let Some(ticks) = switch.tick(player) {
                self.rocket.request_scene_stop(ticks);
            }
        }
    }
}
