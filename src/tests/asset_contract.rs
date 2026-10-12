use super::*;

pub(super) fn run() {
    explicit_roles();
    invalid_metadata();
    #[cfg(feature = "local-assets")]
    legacy_roles();
}

#[cfg_attr(test, test)]
fn explicit_roles() {
    let json = r#"{"pjv_part":{"version":1,"role":"worn_cap","track":"Cap","pupil":null,"surface":"ordinary","stain":"cap","exported":true,"metal":false}}"#;
    let p = decode::<Part>(json, "pjv_part").unwrap().unwrap();
    assert_eq!(p.role, VisibilityRole::WornCap);
    assert_eq!(p.stain, Stain::Cap);
    assert_eq!(p.track.as_deref(), Some("Cap"));
    let b = decode::<Bone>(
        r#"{"pjv_bone":{"version":1,"key":["source_hip","source_parent"],"leg":true,"hip":true}}"#,
        "pjv_bone",
    )
    .unwrap()
    .unwrap();
    assert!(b.leg && b.hip);
}
#[cfg_attr(test, test)]
fn invalid_metadata() {
    assert!(decode::<Part>("{}", "pjv_part").unwrap().is_none());
    for json in [
        "invalid",
        r#"{"pjv_part":{"version":2}}"#,
        r#"{"pjv_part":{"version":1,"role":"typo"}}"#,
        r#"{"pjv_part":{"version":1,"role":"track","track":null}}"#,
    ] {
        assert!(decode::<Part>(json, "pjv_part").is_err());
    }
}

#[cfg(feature = "local-assets")]
#[cfg_attr(test, test)]
fn legacy_roles() {
    let cases: serde_json::Value = serde_json::from_str(r#"[{"name":"EyeClose__EyeMT","part":{"version":1,"role":"hidden","track":null,"pupil":null,"surface":"ordinary","stain":"none","exported":true,"metal":true}},{"name":"EyeHalf__EyeMT","part":{"version":1,"role":"hidden","track":null,"pupil":null,"surface":"ordinary","stain":"none","exported":true,"metal":true}},{"name":"EyeOpen__EyeMT","part":{"version":1,"role":"visible","track":null,"pupil":null,"surface":"ordinary","stain":"none","exported":true,"metal":true}},{"name":"EyelidCloseLeft__EyeMT","part":{"version":1,"role":"closed_left","track":null,"pupil":null,"surface":"ordinary","stain":"none","exported":true,"metal":true}},{"name":"EyelidCloseRight__EyeMT","part":{"version":1,"role":"closed_right","track":null,"pupil":null,"surface":"ordinary","stain":"none","exported":true,"metal":true}},{"name":"Eyeball__EyePupilLeftMT","part":{"version":1,"role":"pupil_left","track":null,"pupil":"EyePupilLeftMT","surface":"ordinary","stain":"none","exported":true,"metal":true}},{"name":"Eyeball__EyePupilRightMT","part":{"version":1,"role":"pupil_right","track":null,"pupil":"EyePupilRightMT","surface":"ordinary","stain":"none","exported":true,"metal":true}},{"name":"EyelidLeft__EyeMT","part":{"version":1,"role":"track","track":"EyelidLeft","pupil":null,"surface":"ordinary","stain":"none","exported":true,"metal":true}},{"name":"Eyeball__EyeMT","part":{"version":1,"role":"hidden","track":null,"pupil":null,"surface":"ordinary","stain":"none","exported":true,"metal":true}},{"name":"MarioEye__EyeMT","part":{"version":1,"role":"unchanged","track":null,"pupil":null,"surface":"ordinary","stain":"none","exported":true,"metal":true}},{"name":"Cap__CapMT","part":{"version":1,"role":"worn_cap","track":"Cap","pupil":null,"surface":"ordinary","stain":"cap","exported":true,"metal":true}},{"name":"CapHair__HairMT","part":{"version":1,"role":"worn_cap","track":"CapHair","pupil":null,"surface":"hair","stain":"cap","exported":true,"metal":true}},{"name":"Hair__HairMT","part":{"version":1,"role":"uncapped_hair","track":null,"pupil":null,"surface":"hair","stain":"body","exported":true,"metal":true}},{"name":"mario_brow_l__HairMT","part":{"version":1,"role":"unchanged","track":null,"pupil":null,"surface":"hair","stain":"body","exported":true,"metal":true}},{"name":"Visor__GlassMT","part":{"version":1,"role":"unchanged","track":null,"pupil":null,"surface":"glass","stain":"none","exported":true,"metal":true}},{"name":"Body__BodyMT","part":{"version":1,"role":"unchanged","track":null,"pupil":null,"surface":"ordinary","stain":"body","exported":true,"metal":true}},{"name":"unrelated","part":{"version":1,"role":"unchanged","track":null,"pupil":null,"surface":"ordinary","stain":"none","exported":false,"metal":true}},{"name":"","part":{"version":1,"role":"unchanged","track":null,"pupil":null,"surface":"ordinary","stain":"none","exported":false,"metal":true}}]"#).unwrap();
    for case in cases.as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let explicit: Part = serde_json::from_value(case["part"].clone()).unwrap();
        assert_eq!(
            crate::legacy_assets::part(name, true),
            explicit,
            "legacy rendering differs for {name}"
        );
    }
}
