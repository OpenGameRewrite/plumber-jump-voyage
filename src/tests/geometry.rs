use super::*;

pub(super) fn run() {
    a_floor_faces_up_and_has_outward_edges();
    floor_below_finds_the_surface();
    json_triangles_are_shifted_to_the_start();
    unregistered_kcl_does_not_support_the_player();
}

#[cfg_attr(test, test)]
fn unregistered_kcl_does_not_support_the_player() {
    let text = r#"{"attributes":[],"objects":[{"model":"VisualWater","collision_enabled":false}],
        "triangles":[[0,0,0,0,0,100,100,0,0,0,1,0,0,0,1,0]]}"#;
    let mesh = TriangleMesh::from_json(text, [0.; 3]).unwrap();
    assert_eq!(mesh.floor_below(10., 10., 50., 0.3), None);
    assert!(mesh.triangles.is_empty());
}

fn floor() -> TriangleMesh {
    TriangleMesh::from_corners(&[
        [
            Vec3::new(-100., 0., -100.),
            Vec3::new(-100., 0., 100.),
            Vec3::new(100., 0., -100.),
        ],
        [
            Vec3::new(100., 0., 100.),
            Vec3::new(100., 0., -100.),
            Vec3::new(-100., 0., 100.),
        ],
    ])
}

#[cfg_attr(test, test)]
fn a_floor_faces_up_and_has_outward_edges() {
    let mesh = floor();
    for t in &mesh.triangles {
        assert!(t.normal.y > 0.99, "{:?}", t.normal);
        let centre = (t.corners[0] + t.corners[1] + t.corners[2]) / 3.;
        for k in 0..3 {
            let from = if k < 2 { t.corners[0] } else { t.corners[1] };
            assert!((centre - from).dot(t.edges[k]) < 0.);
            assert!((t.edges[k].length() - 1.).abs() < 1e-5);
        }
    }
}

#[cfg_attr(test, test)]
fn floor_below_finds_the_surface() {
    let mesh = floor();
    assert_eq!(mesh.floor_below(10., 10., 50., 0.3), Some(0.));
    assert_eq!(mesh.floor_below(500., 0., 50., 0.3), None);
    assert_eq!(mesh.floor_below(10., 10., -1., 0.3), None);
}

/// The export's format: attributes table, objects and 16-number rows.
#[cfg_attr(test, test)]
fn json_triangles_are_shifted_to_the_start() {
    let text = r#"{"attributes":[{"FloorCode":"Ground","MaterialCode":"LawnCap","WallCode":"Wall",
        "CameraCode":"CameraNoThrough","FloorCodeIndex":8,"MaterialCodeIndex":17,"WallCodeIndex":12,"CameraCodeIndex":2}],
        "objects":[{"model":"CapWorldHomeGround000"}],
        "triangles":[[4700,-1050,22150, 4700,-1050,22350, 4900,-1050,22150, 0,1,0, 0,0,7,3]]}"#;
    let mesh = TriangleMesh::from_json(text, Vec3::new(4800., -1050., 22250.)).unwrap();
    assert_eq!(mesh.triangles.len(), 1);
    let t = &mesh.triangles[0];
    assert_eq!(t.corners[0], Vec3::new(-100., 0., -100.));
    assert_eq!(t.kcl_index, 7);
    assert_eq!(mesh.model_of(0), "CapWorldHomeGround000");
    let codes = mesh.codes_of(0);
    assert_eq!(
        (codes.floor.as_str(), codes.material.as_str()),
        ("Ground", "LawnCap")
    );
    assert_eq!((codes.floor_index, codes.material_index), (8, 17));
    assert_eq!(mesh.floor_below(0., 0., 5., 0.3), Some(0.));
}
