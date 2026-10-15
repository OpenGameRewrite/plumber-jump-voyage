use super::*;

pub(super) fn run() {
    scripts();
    scenarios();
    camera_and_buttons();
    lockstep();
    command_line();
}

const IDLE: &str = "start 0 0 0 0\n60 0 0 -\n";
const RUN: &str = "start 0 0 0 0\n120 0 1 -\n60 0 0 -\n";
const JUMP: &str = "start 0 0 0 0\n10 0 0 -\n21 0 0 B\n60 0 0 -\n";

fn options(game: Option<&TraceFile>) -> Options<'_> {
    Options {
        camera: game,
        lockstep: None,
        table: None,
    }
}
fn trace(script: &str) -> Vec<Vec<String>> {
    let script = Script::parse(script).unwrap();
    replay(&script, options(None)).unwrap().rows
}
fn cell<'a>(rows: &'a [Vec<String>], frame: usize, column: &str) -> &'a str {
    let at = COLUMNS.iter().position(|c| *c == column).unwrap();
    &rows[frame][at]
}
fn number(rows: &[Vec<String>], frame: usize, column: &str) -> f32 {
    cell(rows, frame, column).parse().unwrap()
}

#[cfg_attr(test, test)]
fn scripts() {
    let script = Script::parse(
        "# comment\nstart 10 20 30 90 # inline\ncamera 45\nshot 5 9\n3 0.5 -1 B+ZL\n2 0 0 -\n",
    )
    .unwrap();
    assert_eq!(script.start, Vec3::new(10., 20., 30.));
    assert_eq!(
        (script.start_yaw_deg, script.camera_yaw_deg),
        (90., Some(45.))
    );
    assert_eq!(script.shots, [5, 9]);
    assert_eq!(script.frame_count(), 5);
    assert_eq!(script.spans[0].buttons, ["B", "ZL"]);
    assert!(Script::parse("1 0 0 -").unwrap_err().contains("start"));
    assert!(
        Script::parse("start 0 0 0 0\n1 0 0 Q")
            .unwrap_err()
            .contains("line 2")
    );
    assert!(Script::parse("start 0 0 0\n").is_err());
    assert!(Script::parse("start 0 0 0 0\nx 0 0 -").is_err());
}

#[cfg_attr(test, test)]
fn scenarios() {
    let idle = trace(IDLE);
    assert_eq!(idle.len(), 60);
    assert!(idle.iter().all(|r| r.len() == COLUMNS.len()));
    assert_eq!(cell(&idle, 59, "state"), "Wait");
    assert_eq!(number(&idle, 59, "py"), 0.);
    assert_eq!(
        cell(&idle, 0, "events"),
        "",
        "no start-up noise in a settled idle"
    );
    // Running straight: forward is +Z at yaw zero, up to the top speed.
    let run = trace(RUN);
    assert_eq!(cell(&run, 119, "state"), "Run");
    assert_eq!(cell(&run, 119, "anim"), "Run");
    assert!((number(&run, 119, "speed_h") - 14.).abs() < 0.01);
    assert!(number(&run, 119, "pz") > 600. && number(&run, 119, "px").abs() < 1e-3);
    assert_eq!(cell(&run, 0, "anim"), "RunStart");
    assert_eq!(cell(&run, 179, "state"), "Wait");
    let events = (0..run.len())
        .map(|f| cell(&run, f, "events"))
        .collect::<Vec<_>>()
        .join(";");
    assert!(events.contains("se:RunL") && events.contains("se:RunR"));
    assert!(
        (0..run.len()).any(|f| cell(&run, f, "interp").contains(':')),
        "a crossfade starts into the run"
    );
    // Jumping: pressed on frame 10, airborne at once, home again at the end.
    let jump = trace(JUMP);
    assert_eq!(cell(&jump, 9, "ground"), "1");
    assert_eq!(cell(&jump, 10, "ground"), "0");
    assert_eq!(cell(&jump, 10, "state"), "Jump");
    assert_eq!(cell(&jump, 10, "in_buttons"), "!B");
    assert_eq!(cell(&jump, 11, "in_buttons"), "B");
    assert!(number(&jump, 10, "vy") > 10. && number(&jump, 20, "py") > 50.);
    assert!(cell(&jump, 10, "events").contains("effect:JumpDust"));
    assert_eq!(cell(&jump, 90, "ground"), "1");
    assert_eq!(cell(&jump, 90, "py"), "0.000000");
    // The same run twice gives the same bytes.
    assert_eq!(trace(JUMP), jump);
}

#[cfg_attr(test, test)]
fn camera_and_buttons() {
    // A camera looking along +X turns stick-up into a +X run (after the turn).
    let east = trace("start 0 0 0 0\ncamera 90\n60 0 1 -\n");
    assert!(number(&east, 59, "px") > 100. && number(&east, 59, "pz").abs() < 30.);
    assert_eq!(cell(&east, 0, "cam_yaw"), "90.000000");
    // The same camera from a game trace's cam_yaw column.
    let game = TraceFile::parse("frame,cam_yaw\n0,\n1,90\n2,90\n").unwrap();
    let script = Script::parse("start 0 0 0 0\n60 0 1 -\n").unwrap();
    let rows = replay(&script, options(Some(&game))).unwrap().rows;
    assert!(number(&rows, 59, "px") > 100. && cell(&rows, 59, "cam_yaw") == "90.000000");
    // Stick right with the camera along +Z walks toward minus X (right-handed world).
    let right = trace("start 0 0 0 0\n40 1 0 -\n");
    assert!(number(&right, 39, "px") < -50.);
    // Start facing sets the idle front; crouch comes from ZL or ZR.
    let facing = trace("start 0 0 0 90\n2 0 0 ZR\n");
    assert!((number(&facing, 0, "fx") - 1.).abs() < 1e-4);
    assert_eq!(cell(&facing, 1, "state"), "Squat/Wait");
    assert_eq!(cell(&facing, 0, "in_buttons"), "!ZR");
    // A and B both jump.
    for button in ["A", "B"] {
        let rows = trace(&format!("start 0 0 0 0\n2 0 0 {button}\n"));
        assert_eq!(cell(&rows, 0, "state"), "Jump", "{button}");
    }
}

#[cfg_attr(test, test)]
fn lockstep() {
    // Feeding a run its own trace back reproduces it: the injected state
    // covers everything the simulation needs on flat ground.
    for script in [RUN, JUMP] {
        let script = Script::parse(script).unwrap();
        let free = replay(&script, options(None)).unwrap();
        let game = TraceFile::parse(&free.to_csv()).unwrap();
        let locked = replay(
            &script,
            Options {
                camera: None,
                lockstep: Some(&game),
                table: None,
            },
        )
        .unwrap();
        for (frame, (a, b)) in free.rows.iter().zip(&locked.rows).enumerate() {
            for column in ["px", "py", "pz", "vx", "vy", "vz", "state", "ground"] {
                let at = COLUMNS.iter().position(|c| *c == column).unwrap();
                let (x, y) = (&a[at], &b[at]);
                let close = match (x.parse::<f32>(), y.parse::<f32>()) {
                    (Ok(x), Ok(y)) => (x - y).abs() < 0.01,
                    _ => x == y,
                };
                assert!(close, "frame {frame} {column}: {x} vs {y}");
            }
        }
    }
}

#[cfg_attr(test, test)]
fn command_line() {
    let args = |list: &[&str]| list.iter().map(|a| a.to_string()).collect::<Vec<_>>();
    // A replay needs somewhere to write.
    assert!(run_cli(&args(&["x", "--replay", "a.input"])).is_err());
    assert!(run_cli(&args(&["x", "--replay"])).is_err());
    assert!(
        run_cli(&args(&[
            "x",
            "--replay",
            "missing.input",
            "--trace",
            "o.csv"
        ]))
        .is_err()
    );
    // Rendered replays need frames to capture.
    let camera = |script: &Script| CameraTrack::new(script, None).unwrap();
    let plain = Script::parse("start 0 0 0 0\n5 0 0 -\n").unwrap();
    assert!(crate::shots::Driver::new(&plain, camera(&plain), "out".into()).is_err());
    let shots = Script::parse("start 0 0 0 0\nshot 3\n5 0 0 -\n").unwrap();
    assert!(crate::shots::Driver::new(&shots, camera(&shots), "out".into()).is_ok());
}
