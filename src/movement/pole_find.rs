//! Where the pole is: `rs::findPoleClimbStartPos`, `findPoleClimbMovePos`,
//! `findPoleClimbTurnPos` and `findPoleClimbFromTopPos`, the fitting routine
//! they share and the pole-top helper, written from the 1.0.0 executable (none
//! of them has decompiled source). A pole is only its collision triangles,
//! identified by their codes (`Pole`, `Pole10`, `Pole20`, `Pole30Plus`; a face
//! coded `NoClimbPole` belongs to a pole too, but may not be climbed); its
//! centre, width and axis are read off them with arrows and swept spheres.
//!
//! Units are the game's (cm), in the replay's frame.
//!
//! Left out: the angle offset the slanted codes give (`getPoleCodeAngleOffset*`),
//! which only tilts the body's joints, the material name of the touched
//! face (it picks footstep sounds) and the moving collision parts
//! (`calcForceMovePower`).
use super::snap::near_zero_or_greater;
use crate::collider::{FaceFilter, Strike, arrow_strikes_filtered, sphere_strikes};
use crate::geometry::{CollisionCodes, TriangleMesh};
use bevy::prelude::Vec3;

/// The pole codes of the floor attribute.
const FLOOR_CODES: [&str; 4] = ["Pole", "Pole10", "Pole20", "Pole30Plus"];
const DOWN: Vec3 = Vec3::NEG_Y;

/// `rs::isCollisionCodePole`: the face belongs to a pole, climbable or not
/// (the probes want the whole pole).
pub(super) fn is_pole(codes: &CollisionCodes) -> bool {
    codes.wall == "NoClimbPole" || FLOOR_CODES.contains(&codes.floor.as_str())
}

/// `rs::isCollisionCodePoleClimb` and its collision-record variants: a pole
/// that may be climbed.
pub(super) fn is_climbable(codes: &CollisionCodes) -> bool {
    codes.wall != "NoClimbPole" && FLOOR_CODES.contains(&codes.floor.as_str())
}

/// The triangle filters the finders pass to `getFirstPolyOnArrow`.
#[derive(Clone, Copy, Debug)]
pub(super) enum Probe {
    /// `TriangleFilterFunc` over `!isCollisionCodePole`: only pole faces.
    PoleOnly,
    /// Pole faces except those whose normal lies within 5 degrees of the
    /// vector (the pole's caps, for the axis).
    PoleSides(Vec3),
    /// `TriangleFilterFunc` over `isCollisionCodePole`: anything but a pole.
    NotPole,
    /// No filter.
    Any,
}

/// `alCollisionUtil::getFirstPolyOnArrow`: the nearest face the arrow strikes
/// that the filter lets through.
pub(super) fn first_poly(
    mesh: &TriangleMesh,
    start: Vec3,
    vector: Vec3,
    probe: Probe,
) -> Option<Strike> {
    arrow_strikes_filtered(mesh, start, vector, FaceFilter::Any)
        .into_iter()
        .filter(|strike| {
            let codes = mesh.codes_of(strike.triangle);
            match probe {
                Probe::PoleOnly => is_pole(codes),
                Probe::PoleSides(up) => is_pole(codes) && up.dot(strike.normal) <= 0.996_194_7,
                Probe::NotPole => !is_pole(codes),
                Probe::Any => true,
            }
        })
        .min_by(|a, b| a.distance.total_cmp(&b.distance))
}

/// `al::tryNormalizeOrZero`: nothing for a vector that is nearly zero.
pub(super) fn try_normalize(v: Vec3) -> Option<Vec3> {
    (v.length_squared() >= 1e-6).then(|| v.normalize())
}
/// `al::verticalizeVec`: `vec` without its part along `vertical`.
pub(super) fn verticalize(vertical: Vec3, vec: Vec3) -> Vec3 {
    vec - vertical * vertical.dot(vec)
}
/// `al::parallelizeVec`: the part of `vec` along `dir`.
fn parallelize(dir: Vec3, vec: Vec3) -> Vec3 {
    dir * dir.dot(vec)
}
/// `al::limitLength`.
fn limit_length(vec: Vec3, limit: f32) -> Vec3 {
    let length = vec.length();
    if length > limit {
        vec * (limit / length)
    } else {
        vec
    }
}
/// `al::isParallelDirection` with the game's default tolerance.
pub(super) fn is_parallel(a: Vec3, b: Vec3) -> bool {
    let c = a.cross(b);
    c.x.abs() <= 0.01 && c.y.abs() <= 0.01 && c.z.abs() <= 0.01
}
/// `al::isFloorPolygon(normal, gravity)`.
pub(super) fn is_floor_polygon(normal: Vec3, gravity: Vec3) -> bool {
    if normal.length_squared() < 1e-6 {
        return false;
    }
    let d = normal.dot(gravity);
    d < 0. && d.abs() >= 0.342_020_15
}
/// `al::isWallPolygon(normal, gravity)`.
pub(super) fn is_wall_polygon(normal: Vec3, gravity: Vec3) -> bool {
    normal.length_squared() >= 1e-6 && normal.dot(gravity).abs() < 0.342_020_15
}
/// `al::checkHitLinePlane`: where the line through `from` along `dir` meets
/// the plane through `plane_at` with `plane_normal`, unless it runs along it.
fn hit_line_plane(from: Vec3, dir: Vec3, plane_at: Vec3, plane_normal: Vec3) -> Option<Vec3> {
    let along = dir.dot(plane_normal);
    if along.abs() < 0.001 {
        return None;
    }
    let t = plane_normal.dot(plane_at - from) / along;
    Some(from + dir * t)
}

/// What a finder found: the pose the body takes on the pole.
#[derive(Clone, Copy, Debug)]
pub(super) struct Fit {
    /// The point on the pole's face the body hangs at.
    pub(super) pos: Vec3,
    /// The way the body faces (into the pole).
    pub(super) front: Vec3,
    /// The pole's axis there.
    pub(super) axis: Vec3,
    /// How wide the touched face is (the two edge-to-edge distances).
    pub(super) width: f32,
}

/// The faces a sphere of `radius` swept from `start` along `mv` touches and
/// can be pushed back by (those that face against the sweep), each once with
/// the point it was first touched at (`alCollisionUtil::checkStrikeSphereMove`:
/// the original steps the sphere by at most its radius or 35 and orders the
/// hits by when they happen; only the set and the points matter here).
fn sphere_sweep(mesh: &TriangleMesh, start: Vec3, mv: Vec3, radius: f32) -> Vec<(u32, Vec3)> {
    let step = radius.min(35.);
    let count = (mv.length() / step).ceil().max(1.) as usize;
    let mut found: Vec<(u32, Vec3)> = Vec::new();
    for i in 0..=count {
        let center = start + mv * (i as f32 / count as f32);
        for (triangle, at) in sphere_strikes(mesh, center, radius) {
            let against = mesh.triangles[triangle as usize].normal.dot(mv) < 0.;
            if against && !found.iter().any(|(seen, _)| *seen == triangle) {
                found.push((triangle, at));
            }
        }
    }
    found
}

/// The sweep from one side of the pole that the fit reads: across the pole
/// along `across`, which side `sign` (+1 from the positive side). Returns the
/// distance from `contact` to the struck face's plane along `across`, and the
/// direction of the pole's axis in that face.
fn fit_side(
    mesh: &TriangleMesh,
    contact: Vec3,
    normal: Vec3,
    across: Vec3,
    axis: Vec3,
    base: Vec3,
    reach: f32,
    radius: f32,
    sign: f32,
) -> Option<(Vec3, Vec3)> {
    let far = reach * 2.5;
    let start = base + across * (reach * sign);
    let sweep = across * (-far * sign);
    // The face most turned toward the side the sweep came from.
    let mut best: Option<(u32, Vec3)> = None;
    let mut best_score = -1.;
    for &(triangle, at) in sphere_sweep(mesh, start, sweep, radius).iter().rev() {
        let t = &mesh.triangles[triangle as usize];
        if !is_pole(mesh.codes_of(triangle))
            || !near_zero_or_greater(t.normal.dot(normal), 0.087_155_744)
        {
            continue;
        }
        let score = sign * t.normal.dot(across);
        if best_score <= score {
            best_score = score;
            best = Some((triangle, at));
        }
    }
    let (triangle, mut face_at) = best?;
    let mut face_normal = mesh.triangles[triangle as usize].normal;
    // A cap (a face along the axis): read the side face from the inside.
    if axis.dot(face_normal) > 0.984_807_73 {
        let from = face_at - face_normal * 10. + across * (reach * sign);
        let vector = across * (-(reach + 10.) * sign);
        if let Some(strike) = first_poly(mesh, from, vector, Probe::PoleOnly) {
            face_at = strike.position;
            face_normal = strike.normal;
        }
    }
    let on_plane = hit_line_plane(contact, across, face_at, face_normal).unwrap_or(face_at);
    let toward = limit_length(parallelize(across, on_plane - contact), far);
    let along = match try_normalize(verticalize(normal, face_normal)) {
        Some(v) => try_normalize(normal.cross(v)).unwrap_or(axis),
        None => axis,
    };
    Some((toward, along))
}

/// The routine every finder ends in (0x560f18): the pole was touched at
/// `contact`, whose face has `normal`; `dir` is the direction the pole runs
/// along, `catch` the catch range and `reach` the longest the pole is
/// expected to be wide. Two sweeps across the pole, from either side, find
/// the faces next to the touched one; where their planes cut the line through
/// the contact along the face gives the face's extent, and the body goes to
/// its middle.
pub(super) fn fit_pole(
    mesh: &TriangleMesh,
    contact: Vec3,
    normal: Vec3,
    dir: Vec3,
    catch: f32,
    reach: f32,
) -> Option<Fit> {
    let across = dir.cross(normal).normalize_or_zero();
    let axis = normal.cross(across).normalize_or_zero();
    let quarter = catch * 0.25;
    let radius = quarter * 0.5;
    let base = contact - normal * quarter;
    let first = fit_side(
        mesh, contact, normal, across, axis, base, reach, radius, 1.,
    );
    let second = fit_side(
        mesh, contact, normal, across, axis, base, reach, radius, -1.,
    );
    let front = -normal;
    match (first, second) {
        (Some((to_a, axis_a)), Some((to_b, axis_b))) => {
            let axis = try_normalize(axis_b + axis_a).unwrap_or(axis);
            let middle = verticalize(axis, (to_a + to_b) * 0.5);
            Some(Fit {
                pos: contact + middle,
                front,
                axis,
                width: to_a.length() + to_b.length(),
            })
        }
        (Some((to_a, axis_a)), None) => Some(Fit {
            pos: contact + to_a - across * quarter,
            front,
            axis: axis_a,
            width: catch,
        }),
        (None, Some((to_b, axis_b))) => Some(Fit {
            pos: contact + to_b + across * quarter,
            front,
            axis: axis_b,
            width: catch,
        }),
        (None, None) => None,
    }
}

/// What the judge offers `findPoleClimbStartPos`: the surface the body
/// touched, the "up" the judge worked out for it and the range limits.
pub(super) struct Touch {
    pub(super) normal: Vec3,
    pub(super) pos: Vec3,
    pub(super) up: Vec3,
}

/// `rs::findPoleClimbStartPos`: may the body hold on to the pole it touched,
/// and where? The pole must reach `margin` above or below the touch.
pub(super) fn find_start_pos(
    mesh: &TriangleMesh,
    touch: &Touch,
    catch: f32,
    reach: f32,
    margin: f32,
) -> Option<Fit> {
    let Touch { normal, up, .. } = *touch;
    let mut contact = touch.pos;
    let mut dir = up;
    if is_floor_polygon(normal, DOWN) {
        // A floor-coded pole face: the pole must lie under the touch.
        let start = contact - normal * reach;
        let vector = normal * reach;
        let hit = first_poly(mesh, start, vector, Probe::PoleOnly)
            .or_else(|| first_poly(mesh, start + up, vector, Probe::PoleOnly))?;
        if -normal.dot(hit.normal) < 0.707_106_77 {
            return None;
        }
    } else if is_wall_polygon(normal, DOWN) {
        let half = catch * 0.5;
        let inside = contact - normal;
        let down = DOWN * (half + 1.);
        let hit = first_poly(mesh, inside - DOWN * half, down, Probe::PoleOnly)
            .or_else(|| first_poly(mesh, inside + DOWN * half, -down, Probe::PoleOnly));
        if let Some(hit) = hit
            && hit.normal.dot(normal) > 0.5
        {
            // The face the arrow met: the pole runs up along it.
            dir = match try_normalize(verticalize(hit.normal, DOWN)) {
                Some(along) => -along,
                None => up,
            };
        }
        // The pole must go on above or below the touch (`PoleClimbUpMargine`).
        let out = contact + normal * half;
        let into = -normal * catch;
        if first_poly(mesh, out + dir * margin, into, Probe::PoleOnly).is_none() {
            let below = first_poly(mesh, out - dir * margin, into, Probe::PoleOnly)?;
            contact = below.position;
        }
    }
    fit_pole(mesh, contact, normal, dir, catch, reach)
}

/// `0x562a0c`: the middle of the shortest segment between two lines (a point
/// and a direction each), when they are not parallel.
fn lines_meet(p0: Vec3, d1: Vec3, q0: Vec3, d2: Vec3) -> Option<Vec3> {
    if is_parallel(d1, d2) {
        return None;
    }
    let c = d1.dot(d2);
    let spread = (1. - c * c).max(0.);
    if spread.abs() < 1e-5 {
        return None;
    }
    let w = q0 - p0;
    let a = w.dot(d1);
    let b = w.dot(d2);
    let t = (a - c * b) / spread;
    let u = (c * a - b) / spread;
    Some(((p0 + d1 * t) + (q0 + d2 * u)) * 0.5)
}

/// What the top finder needs: the actor's own axes.
pub(super) struct Actor {
    pub(super) front: Vec3,
    pub(super) up: Vec3,
    pub(super) side: Vec3,
}

/// The pole's top (0x562100): the body at `base`, the pole running away along
/// `dir` from the body (`width` across); `up` is the direction the top lies
/// in and `height` how far up to look. Returns the pose on the top cap, and
/// the distance the original reports as the width.
fn find_top(
    mesh: &TriangleMesh,
    actor: &Actor,
    dir: Vec3,
    height: f32,
    width: f32,
    up: Vec3,
    base: Vec3,
    base_is_output: bool,
) -> Option<Fit> {
    let t = width * 4.;
    let start = base + dir * t;
    let vector = -dir * t;
    let across = first_poly(mesh, start, vector, Probe::PoleOnly)
        .or_else(|| first_poly(mesh, start - actor.side * 2., vector, Probe::PoleOnly))?;
    // Down from above the middle of the pole.
    let middle = (across.position + base) * 0.5 + up * height;
    let down = -up * (height + 2.);
    let cap = first_poly(mesh, middle, down, Probe::PoleOnly)
        .or_else(|| first_poly(mesh, middle - actor.side * 2., down, Probe::PoleOnly))?;
    if !is_climbable(mesh.codes_of(cap.triangle)) || !is_floor_polygon(cap.normal, -up) {
        return None;
    }
    // The width is measured to the output position, which is the base itself
    // when the caller passed its output as the base.
    let from = if base_is_output { cap.position } else { base };
    Some(Fit {
        pos: cap.position,
        front: -cap.normal,
        axis: Vec3::Y,
        width: (across.position - from).length(),
    })
}

/// What `findPoleClimbMovePos` found.
pub(super) struct Moved {
    pub(super) fit: Fit,
    /// The pole ends within the margin: the pose is its top.
    pub(super) top: bool,
}

/// `rs::findPoleClimbMovePos`: the body at `target` goes `speed` along the
/// pole's axis (up when positive); where does it end? `width` is the pole's
/// width the state last measured, `margin` how much pole must remain above
/// for it to count as going on.
pub(super) fn find_move_pos(
    mesh: &TriangleMesh,
    actor: &Actor,
    target: Vec3,
    speed: f32,
    margin: f32,
    width: f32,
    reach: f32,
) -> Option<Moved> {
    let step = actor.up * speed;
    // Looking back down the pole from where the move ends, a face that is
    // not square to the axis bends the way (the pole is slanted).
    let ahead = target + step + actor.front * (width * 0.25);
    let mut disp = step;
    if let Some(hit) = first_poly(mesh, ahead, -step, Probe::PoleOnly)
        && let Some(across) = try_normalize(verticalize(actor.front, hit.normal))
    {
        let square = actor.up.dot(across).abs();
        if square < 0.866_025_4
            && let Some(slide) = try_normalize(verticalize(across, step))
        {
            let along = (ahead - hit.position).dot(slide);
            let out = -0.5 * square * width;
            let bent = hit.position + across * out + slide * along;
            disp = bent - target;
        }
    }
    // The pole's side at the new place, seen from 50 out in front of it.
    let start = target + disp - actor.front * 50.;
    let vector = actor.front * 100.;
    let probe = Probe::PoleSides(actor.up);
    let side = first_poly(mesh, start, vector, probe)
        .or_else(|| first_poly(mesh, start - actor.side * 2., vector, probe));
    let Some(hit) = side else {
        // No pole there: going up, it is the top of the pole.
        if speed <= 0. {
            return None;
        }
        let top = find_top(
            mesh,
            actor,
            actor.front,
            speed + 5.,
            width,
            actor.up,
            target,
            false,
        )?;
        return Some(Moved { fit: top, top: true });
    };
    if !is_climbable(mesh.codes_of(hit.triangle)) {
        return None;
    }
    let fit = fit_pole(mesh, hit.position, hit.normal, actor.up, width, reach)?;
    if speed <= 0. {
        return Some(Moved { fit, top: false });
    }
    // Going up the pole must go on for the margin above the new place, or
    // this is its top.
    let half = fit.width * 0.5;
    let start = fit.pos - fit.front * half + fit.axis * margin;
    if first_poly(mesh, start, fit.front * fit.width, Probe::PoleOnly).is_some() {
        return Some(Moved { fit, top: false });
    }
    match find_top(
        mesh,
        actor,
        fit.front,
        margin,
        fit.width,
        fit.axis,
        fit.pos,
        true,
    ) {
        Some(top) => Some(Moved { fit: top, top: true }),
        None => Some(Moved { fit, top: false }),
    }
}

/// What the turn finder is told besides the pole's width.
pub(super) struct TurnAt {
    /// Where the body stands now (`held`) and the point the turn aims for.
    pub(super) held: Vec3,
    pub(super) target: Vec3,
    /// The direction round the pole the turn goes (sideways, away from the
    /// stick).
    pub(super) direction: Vec3,
}

/// `rs::findPoleClimbTurnPos`: the body goes round the pole toward
/// `turn.target`; where on the pole does it end, and is there room?
/// `slack` is how far in from the face the sideways probe starts.
pub(super) fn find_turn_pos(
    mesh: &TriangleMesh,
    actor: &Actor,
    turn: &TurnAt,
    width: f32,
    slack: f32,
    reach: f32,
) -> Option<Fit> {
    let front = actor.front;
    let target = turn.target;
    let start = target - front * 5.;
    let vector = front * 55.;
    let mut slack = slack;
    let mut hit = first_poly(mesh, start, vector, Probe::PoleOnly);
    // A face straight ahead is read as it is; one that is not square to the
    // body, or none, sends the probe round the pole toward the turn.
    let direct = match &hit {
        Some(h) => !is_parallel(-front, h.normal),
        None => {
            slack = 0.;
            false
        }
    };
    if !direct {
        // From the axis (half a width in), back along the turn, toward it.
        let centre = target + front * (width * 0.5);
        let from = centre - turn.direction * slack;
        let probe = turn.direction * (slack + 50.);
        hit = first_poly(mesh, from, probe, Probe::PoleOnly)
            .or_else(|| first_poly(mesh, from - front * 2., probe, Probe::PoleOnly));
    }
    if std::env::var("PJV_DBG").is_ok() {
        eprintln!("turn: target {:?} front {:?} direct {} hit {:?} width {} slack {}", target, front, direct, hit.as_ref().map(|h| (h.triangle, h.position, h.normal)), width, slack);
    }
    let hit = hit?;
    if !is_climbable(mesh.codes_of(hit.triangle)) {
        return None;
    }
    let mut fit = fit_pole(mesh, hit.position, hit.normal, actor.up, width, reach)?;
    if std::env::var("PJV_DBG").is_ok() {
        eprintln!("turn fit {:?}", fit);
    }
    if !is_parallel(turn.direction, fit.front) {
        // The face turned away from the way round: put the point where the
        // line along the old axis meets the one along the old front.
        let by_axis = lines_meet(turn.held, actor.up, fit.pos, fit.axis);
        let by_front = lines_meet(turn.held, front, fit.pos, fit.front);
        if let (Some(a), Some(b)) = (by_axis, by_front)
            && let Some(d) = try_normalize(a - b)
        {
            let along = d.dot(fit.axis);
            if along.abs() >= 0.001 {
                let t = (turn.held - fit.pos).dot(d) / along;
                fit.pos += fit.axis * t;
            }
        }
    }
    // Nothing but the pole may stand in the way, at the body's height or
    // 100 above it.
    let length = fit.width + 5.;
    let blocked = |from: Vec3| {
        first_poly(mesh, from, front * length, Probe::NotPole).is_some()
    };
    if blocked(start) || blocked(start + actor.up * 100.) {
        return None;
    }
    Some(fit)
}

/// `rs::findPoleClimbFromTopPos`: from the top, the body goes to the pole's
/// face below `target`.
pub(super) fn find_from_top_pos(
    mesh: &TriangleMesh,
    actor: &Actor,
    target: Vec3,
    width: f32,
    reach: f32,
) -> Option<Fit> {
    let start = target - actor.front * 50.;
    let vector = actor.front * 100.;
    let hit = first_poly(mesh, start, vector, Probe::Any)
        .or_else(|| first_poly(mesh, start - actor.side * 2., vector, Probe::Any))?;
    if !is_climbable(mesh.codes_of(hit.triangle)) {
        return None;
    }
    fit_pole(mesh, hit.position, hit.normal, actor.up, width, reach)
}
