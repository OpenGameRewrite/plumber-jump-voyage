use super::*;

pub(super) fn run() {
    voice_names_and_triggers();
    event_cues();
    cursors();
    layer_levels();
}

fn sample_group() -> Group {
    let mut group = Group::default();
    group.actions.insert(
        "Jump".into(),
        vec![
            Trigger {
                play: "Jump".into(),
                frame: -1.,
                once: false,
            },
            Trigger {
                play: "vJump".into(),
                frame: 3.,
                once: false,
            },
        ],
    );
    group
        .reactions
        .insert("Land".into(), vec!["Land".into(), "LandMat".into()]);
    group
}

#[cfg_attr(test, test)]
fn voice_names_and_triggers() {
    assert!(is_voice("vJump") && !is_voice("RunL") && !is_voice("vague") && !is_voice("v"));
    let mut group = sample_group();
    // A clip that is just starting plays the unset-frame sounds; later ones wait.
    assert_eq!(group.triggered("Jump", -1., 1., false), ["Jump"]);
    assert_eq!(group.triggered("Jump", 1., 3., false), ["vJump"]);
    assert!(
        group.triggered("Jump", 3., 5., false).is_empty(),
        "each trigger plays once"
    );
    assert!(group.triggered("Wait", -1., 5., false).is_empty());
    // A looped clip replays its repeating sounds but not the one-time ones.
    group.actions.get_mut("Jump").unwrap()[1].once = true;
    assert_eq!(group.triggered("Jump", -1., 3., false), ["Jump", "vJump"]);
    assert_eq!(group.triggered("Jump", -1., 3., true), ["Jump"]);
    assert_eq!(group.reaction_plays("Land"), ["Land", "LandMat"]);
    assert!(group.reaction_plays("Nothing").is_empty());
}

#[cfg_attr(test, test)]
fn event_cues() {
    let on = |mode| FxContext { mode };
    let mut boosts = 0;
    let land = |mode, boosts: &mut u32| fx_cues(Fx::Land { hard: false }, on(mode), boosts);
    let reaction = |name| [Cue::Reaction { cap: false, name }];
    // The state that takes over after the landing names the reaction.
    assert_eq!(land(Move::Run, &mut boosts), reaction("LandRun"));
    assert_eq!(land(Move::Jump, &mut boosts), reaction("LandRun"));
    assert_eq!(land(Move::Idle, &mut boosts), reaction("Land"));
    assert_eq!(land(Move::Roll, &mut boosts), reaction("RollLand"));
    assert!(land(Move::Crouch, &mut boosts).is_empty());
    assert_eq!(
        fx_cues(Fx::PoundLand, on(Move::PoundLand), &mut boosts),
        reaction("HipDropLand")
    );
    // A footfall starts the step, its material variant and the rustle; the
    // crawl's steps come from its clip, the swim's are plain.
    assert_eq!(
        fx_cues(Fx::Step { left: true }, on(Move::Run), &mut boosts),
        [
            Cue::Rated("RunL"),
            Cue::Rated("RunMatL"),
            Cue::Rated("ShakeNoiseL")
        ]
    );
    assert_eq!(
        fx_cues(Fx::FootLift { left: false }, on(Move::Run), &mut boosts),
        [Cue::Rated("RunUpR")]
    );
    assert!(fx_cues(Fx::Step { left: true }, on(Move::Crawl), &mut boosts).is_empty());
    assert_eq!(
        fx_cues(Fx::Step { left: false }, on(Move::Swim), &mut boosts),
        [Cue::Play("SwimRunR")]
    );
    assert_eq!(
        fx_cues(Fx::RollSpin, on(Move::Roll), &mut boosts),
        [Cue::Play("Roll")]
    );
    assert_eq!(
        fx_cues(Fx::FallWhoosh { frames: 2 }, on(Move::Fall), &mut boosts),
        [Cue::Hold("FallWhoosh_loop")]
    );
    // The first roll boost sounds different from the ones after it.
    let boost = Fx::RollBoost { perfect: false };
    let name = |cues: Vec<Cue>| match cues[0] {
        Cue::Reaction { name, .. } => name,
        other => panic!("{other:?}"),
    };
    let roll = on(Move::Roll);
    assert_eq!(name(fx_cues(boost, roll, &mut boosts)), "RollBoost");
    assert_eq!(name(fx_cues(boost, roll, &mut boosts)), "RollBoostAgain");
    fx_cues(Fx::RollStart, roll, &mut boosts);
    assert_eq!(name(fx_cues(boost, roll, &mut boosts)), "RollBoost");
    // The cap's throw raises its reaction, a voice reaction by the throw's count,
    // and the stay's steps a level sound of the cap's own.
    assert_eq!(
        fx_cues(Fx::Throw(Throw::Down), on(Move::Throw), &mut boosts),
        [Cue::Reaction {
            cap: true,
            name: "Throw"
        }]
    );
    assert_eq!(
        fx_cues(Fx::ThrowVoice(3), on(Move::Throw), &mut boosts),
        [Cue::Reaction {
            cap: true,
            name: "ThrowVoice3"
        }]
    );
    assert_eq!(
        fx_cues(Fx::CapStay, on(Move::Idle), &mut boosts),
        [Cue::CapPlay("PgStayLv")]
    );
    assert!(fx_cues(Fx::Skid, on(Move::Skid), &mut boosts).is_empty());
}

#[cfg_attr(test, test)]
fn cursors() {
    let mut cursor = AnimCursor::default();
    let first = cursor.advance("Jump", 1, 1., false);
    assert!(first.restarted);
    assert_eq!(
        first.scan,
        Some((-1., 1., true)),
        "a new clip scans from its start"
    );
    let next = cursor.advance("Jump", 1, 2., false);
    assert!(!next.restarted);
    assert_eq!(next.scan, Some((1., 2., true)));
    let again = cursor.advance("Jump", 2, 1., false);
    assert!(again.restarted, "a new serial restarts the same clip");
    // A loop that wrapped replays its repeating sounds for Mario only.
    assert!(!cursor.advance("Wait", 3, 50., true).replay);
    let wrapped = cursor.advance("Wait", 3, 4., true);
    assert_eq!(wrapped.scan, Some((-1., 4., false)));
    // From then on the clip's one-time sounds are spent, until it restarts.
    assert!(wrapped.replay);
    assert!(cursor.advance("Wait", 3, 5., true).replay);
    assert!(!cursor.advance("Wait", 4, 1., true).replay);
    assert_eq!(cursor.advance("Wait", 4, 0.5, false).scan, None);
    assert_eq!(cap_action_name("FlyingWaitR"), "FlyingWait");
    assert_eq!(cap_action_name("FlyingWaitSpiral"), "FlyingWaitSpiral");
    assert_eq!(cap_action_name("StayL"), "StayL");
}

#[cfg_attr(test, test)]
fn layer_levels() {
    let table: Table = serde_json::from_str(
        r#"{"resources":{"R":{"volume":0.5,"random_semitones":[-2,2]}},
            "mario":{"plays":{"P":{"layers":[{"resource":"R","pitch":12,"random_pitch":1,
            "random_volume":0.5,"volume_curve":[0,10,0.5,1]}]}}}}"#,
    )
    .unwrap();
    let layer = &table.mario.plays["P"].layers[0];
    let resource = &table.resources["R"];
    // Random draws at their middle leave the pitch at the layer's own octave.
    let mut draws = [0.5, 0.5, 0.].into_iter();
    let (gain, pitch) = layer.levels(resource, 10., || draws.next().unwrap());
    assert!(
        (gain - 0.5).abs() < 1e-6 && (pitch - 2.).abs() < 1e-5,
        "{gain} {pitch}"
    );
    let (quiet, _) = layer.levels(resource, 0., || 0.);
    assert!((quiet - 0.25).abs() < 1e-6, "speed scales the gain");
}
