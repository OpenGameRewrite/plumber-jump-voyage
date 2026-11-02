use super::*;

pub(super) fn run() {
    only_an_entry_finds_the_surface();
    an_ignored_start_area_shoots_back_from_the_end();
    an_ignored_end_area_finds_nothing();
}

/// A pool (surface at 100, 1000 deep) and a one-sided ignored area stacked over it (100..300).
fn pool_with_ignored_cap() -> WaterAreas {
    let mut areas = WaterAreas::demo_box(0., 0., 500., 500., 100., 1000.);
    let mut cap = WaterAreas::demo_box(0., 0., 500., 500., 300., 200.).areas;
    cap[0].ignore = true;
    areas.areas.extend(cap);
    areas
}

/// 0x545008 needs `isInWaterPos(start)` false and `isInWaterPos(start + offset)` true: from above
/// into the pool finds the top face, the same arrow leaving the pool finds nothing.
#[cfg_attr(test, test)]
fn only_an_entry_finds_the_surface() {
    let pool = WaterAreas::demo_box(0., 0., 500., 500., 100., 1000.);
    let above = Vec3::new(0., 150., 0.);
    let down = Vec3::new(0., -100., 0.);
    let (position, normal) = pool.surface_by_arrow(above, down).expect("an entry");
    assert_eq!((position.y, normal), (100., Vec3::Y));
    assert!(pool.surface_by_arrow(above + down, -down).is_none());
    // Both ends in the water, or both out of it.
    assert!(pool.surface_by_arrow(Vec3::new(0., 0., 0.), down).is_none());
    assert!(
        pool.surface_by_arrow(above, Vec3::new(0., 10., 0.))
            .is_none()
    );
}

/// The start inside an ignored area (no water) with the end in the pool below it: the arrow
/// runs from the end back up to the start against the ignored area, whose floor normal (down)
/// comes out reversed (up).
#[cfg_attr(test, test)]
fn an_ignored_start_area_shoots_back_from_the_end() {
    let areas = pool_with_ignored_cap();
    let start = Vec3::new(0., 200., 0.);
    let (position, normal) = areas
        .surface_by_arrow(start, Vec3::new(0., -150., 0.))
        .expect("an entry through the ignored area's floor");
    assert_eq!((position.y, normal), (100., Vec3::Y));
}

/// An end inside an ignored area is no water: nothing is found.
#[cfg_attr(test, test)]
fn an_ignored_end_area_finds_nothing() {
    let areas = pool_with_ignored_cap();
    let start = Vec3::new(0., 400., 0.);
    assert!(
        areas
            .surface_by_arrow(start, Vec3::new(0., -200., 0.))
            .is_none()
    );
}
