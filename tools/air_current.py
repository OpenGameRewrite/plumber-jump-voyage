"""AirCurrent (obj265) contact evidence: the wind source's sensor traffic with Mario, judged against source.

The Snow balcony current (SnowWorldTownStage scenario 1, zone SnowWorldBalconyZone, obj265) overlaps
every sensor of Mario for the whole recording and sends him `rs::SensorMsgByugoBlow`. The probe lists
every overlap as contamination. This module proves what the traffic does, from the executable, and
never from the recording's own outcome:

* `AirCurrent::attackSensor` (0x212e40) sends only while the nerve is Blow (rate = clamp(step / 20),
  `calcNerveRate` 20 steps) or BlowEnd, when the sensors overlap, the receiver is an enemy-attack
  sensor and the vector of the force helper (0x212fe0) is not near zero. The helper takes the receiving
  sensor's centre in the current's frame (`al::multVecInvQuat`, ActorPoseUtil.cpp:420), zeroes it inside
  the near-origin cone, tapers it over the last 400 of the length and across 250..400 off the axis, and
  scales it by 12 and the rate. Everything is evaluated here in f32 from the placement alone.
* Mario's receiveMsg (0x428810) refuses a PlayerAttack sensor (Carry, HipDropKnockDown: 0x428844) and
  otherwise forwards to `PlayerExternalVelocity::receiveMsgPlayer` (0x4289d8), which accepts the message
  unless Mario is dead, bound, or in Demo/Hack/Abyss. So Foot/Body/Head answer true and the two attack
  sensors answer false.
* The accepted forces enter `ExternalForceKeeper` (0x551e40: componentwise extrema, force = min + max) and
  `PlayerExternalVelocity::update` (0x440dec): v = v * drag + force * 0.275, zeroed below length 0.1.

Observations only validate the simulation, they never drive it. Per frame this checks:

1. the recorded actor row (placement identity, pose, constant arguments, Blow nerve and rate, one step per
   frame), the recorded calls (the sensors, order, force bits and answers the source predicts from Mario's
   pose, and Mario's sensor positions) and the recorded external velocity (the update formula on the recorded
   calls);
2. the rewrite's own row: its source pose is the placement's, its nerve and step are the recorded ones (the
   rewrite starts the chain itself, nothing is seeded, so a wrong `relax` shows up here), the sensors it
   evaluated and the forces it sent are what the source predicts from the rewrite's own sensor centres, and
   its external velocity follows the update formula on those sends.

Timing. The probe flushes a sample's calls when the sample is taken, and reads the actor's nerve step after that
frame's actor update. So the calls of sample f come from the sensor phase of frame f-1: they read Mario's pose of
row f-1 and the nerve step of row f minus one (the rewrite's row f holds that same step). The probe's counters
pin this independently of the trajectory: the first sample's buffer (kept + dropped calls) holds five calls
(one per sensor Mario overlaps) for every phase from Blow step 1 up to the step before the sampled one, so the
sampled step is that count plus one; `verify` refuses a recording whose counters say otherwise. The nerve chain's
length before Blow (Sleep 1 + BlowStart 21 updates, 0x213270/0x213550) turns that count into the number of
updates the actor ran before the sample, which is what the replay's `relax` has to reproduce (see
docs/fidelity-harness.md).
"""
import math
import struct

STAGE, SCENARIO = 'SnowWorldTownStage', 1
ZONE_NAME, ZONE_ID, ID = 'SnowWorldBalconyZone', 'obj534', 'obj265'
HOST_NAME = '気流発生オブジェ'
# Mario's sensors in registry order: name, offset up from the actor, radius (PlayerActorHakoniwa InitSensor),
# and whether `receiveMsg` answers true (the PlayerAttack sensors are refused before the message is looked at).
SENSORS = (('Foot', 40., 40., True), ('Body', 75., 50., True), ('Head', 110., 40., True),
           ('Carry', 100., 130., False), ('HipDropKnockDown', 0., 150., False))
# Sensors the rewrite evaluates (the two attack sensors only ever answer false).
NATIVE_SENSORS = SENSORS[:3]
# States in which receiveMsg resets the external velocity and answers false instead (0x428810).
REFUSING_STATES = ('Dead', 'Bind', 'Hack', 'Demo', 'Abyss')
MESSAGE = 'rs::SensorMsgByugoBlow'
SENSITIVITY, GROUND_DRAG, AIR_DRAG = 0.275, 0.95, 0.955


def f32(x):
    return struct.unpack('<f', struct.pack('<f', x))[0]


def bits(cell, sep):
    tokens = cell.split(sep)
    if len(tokens) < 3 or any(len(t.removeprefix('0x')) != 8 for t in tokens):
        raise ValueError('float bits')
    return [struct.unpack('>f', bytes.fromhex(t.removeprefix('0x')))[0] for t in tokens]


def single(token):
    return struct.unpack('>f', bytes.fromhex(token.removeprefix('0x')))[0]


def hex_bits(values):
    return '/'.join(f'{struct.unpack(">I", struct.pack(">f", v))[0]:08x}' for v in values)


def add(a, b):
    return [f32(x + y) for x, y in zip(a, b)]


def length_squared(a):
    return f32(f32(f32(a[0] * a[0]) + f32(a[1] * a[1])) + f32(a[2] * a[2]))


# ---- the source ------------------------------------------------------------------------------------------

def quat_from_degrees(rotate):
    """`ActorPoseKeeperTQGSV::updatePoseRotate` 0x8eda54: the placement's rotation as sead builds the quaternion."""
    half = [f32(f32(f32(d) * f32(0.017453292)) * 0.5) for d in rotate]
    sx, sy, sz = (f32(math.sin(h)) for h in half)
    cx, cy, cz = (f32(math.cos(h)) for h in half)
    m = lambda *t: f32(f32(t[0] * t[1]) * t[2])
    w = f32(m(cx, cy, cz) + m(sx, sy, sz))
    x = f32(m(cz, cy, sx) - m(cx, sz, sy))
    y = f32(m(cx, cz, sy) + m(cy, sz, sx))
    z = f32(m(cx, cy, sz) - m(cz, sy, sx))
    return [x, y, z, w]


def front_of(q):
    """The third column of the pose matrix (`al::calcFrontDir`)."""
    x, y, z, w = q
    return [f32(f32(2. * f32(x * z)) + f32(2. * f32(w * y))),
            f32(f32(2. * f32(y * z)) - f32(2. * f32(w * x))),
            f32(f32(1. - f32(2. * f32(x * x))) - f32(2. * f32(y * y)))]


def local_point(translate, q, point):
    """`al::multVecInvQuat`: the point in the actor's frame, with the decomp's operation order."""
    v = [f32(point[i] - translate[i]) for i in range(3)]
    qx, qy, qz, qw = q
    rx = f32(f32(f32(-f32(qy * v[2])) + f32(qz * v[1])) + f32(qw * v[0]))
    ry = f32(f32(f32(qx * v[2]) - f32(qz * v[0])) + f32(qw * v[1]))
    rz = f32(f32(f32(-f32(qx * v[1])) + f32(qy * v[0])) + f32(qw * v[2]))
    rw = f32(f32(f32(qx * v[0]) + f32(qy * v[1])) + f32(qz * v[2]))
    return [f32(f32(f32(f32(rx * qw) + f32(ry * qz)) - f32(rz * qy)) + f32(rw * qx)),
            f32(f32(f32(f32(-f32(rx * qz)) + f32(ry * qw)) + f32(rz * qx)) + f32(rw * qy)),
            f32(f32(f32(f32(rx * qy) - f32(ry * qx)) + f32(rz * qw)) + f32(rw * qz))]


def lerp_value(a, b, t, clamp_a, clamp_b):
    """`al::lerpValue(a, b, t, clampA, clampB)` (MathUtil.cpp:566)."""
    if abs(f32(t - b)) < f32(0.001):
        return clamp_a if a <= b else clamp_b
    rate = min(max(f32(f32(a - b) / f32(t - b)), 0.), 1.)
    return f32(f32(clamp_a * f32(1. - rate)) + f32(rate * clamp_b))


class Source:
    """The placed current: pose, length and sensor sphere from the placement alone (`AirCurrent::init` 0x212b90)."""

    def __init__(self, placement):
        self.translate = [f32(v) for v in placement['translate']]
        self.quat = quat_from_degrees(placement['rotate'])
        self.length = f32(f32(placement['scale'][2]) * 2000.)
        self.front = front_of(self.quat)
        self.radius = f32(f32(0.55) * self.length)
        self.centre = [f32(self.translate[i] + f32(self.front[i] * self.radius)) for i in range(3)]

    def overlaps(self, centre, radius):
        reach = f32(self.radius + radius)
        delta = [f32(centre[i] - self.centre[i]) for i in range(3)]
        return length_squared(delta) <= f32(reach * reach)

    def force(self, point, rate):
        """The force helper 0x212fe0 at one sensor centre."""
        local = local_point(self.translate, self.quat, point)
        if local[2] < 200.:
            cone = f32(f32(local[2] / 200.) * 400.)
            if f32(cone * cone) < f32(f32(local[0] * local[0]) + f32(local[1] * local[1])):
                return [0., 0., 0.]
        axial = lerp_value(local[2], f32(self.length - 400.), self.length, 1., 0.)
        across = max(abs(local[0]), abs(local[1]))
        transverse = lerp_value(across, 250., 400., 1., 0.)
        out = []
        for component in self.front:
            scaled = f32(transverse * f32(axial * f32(component * 12.)))
            out.append(f32(scaled * rate))
        return out

    def sent(self, centre, radius, rate):
        """`attackSensor` 0x212e40 against one receiving sensor: the force it sends, or None."""
        if abs(rate) < f32(0.001) or not self.overlaps(centre, radius):
            return None
        force = self.force(centre, rate)
        return None if length_squared(force) <= f32(f32(0.001) * f32(0.001)) else force


def blow_rate(step):
    """`calcNerveRate(20)` (Blow, BlowForceCalc): clamp(step / 20, 0, 1)."""
    return f32(min(max(f32(step / 20.), 0.), 1.))


def sensor_centre(pose, offset):
    """Mario's sensor centre: his translation plus `up * offset`."""
    position, up = pose
    return [f32(position[i] + f32(up[i] * offset)) for i in range(3)]


def keeper_force(forces):
    """`ExternalForceKeeper` (0x551e40, calcForce 0x551f64): componentwise extrema from zero, summed."""
    low, high = [0.] * 3, [0.] * 3
    for force in forces:
        low = [low[i] if low[i] < force[i] else force[i] for i in range(3)]
        high = [high[i] if high[i] > force[i] else force[i] for i in range(3)]
    return add(low, high)


def external_step(previous, force, on_ground):
    """`PlayerExternalVelocity::update` (0x440dec): v = v * drag + force * 0.275, zeroed below length 0.1."""
    drag = f32(GROUND_DRAG if on_ground else AIR_DRAG)
    out = [f32(f32(previous[i] * drag) + f32(force[i] * f32(SENSITIVITY))) for i in range(3)]
    return [0.] * 3 if length_squared(out) < f32(f32(0.1) * f32(0.1)) else out


# ---- the recorded and the replayed data --------------------------------------------------------------------

def gate(geometry):
    """The exported placement this evidence covers (movement/air_current.rs), or None."""
    if not geometry or (geometry.get('stage'), geometry.get('scenario')) != (STAGE, SCENARIO):
        return None
    sources = geometry.get('air_currents', [])
    if len(sources) != 1:
        return None
    source = sources[0]
    if (source.get('id') == ID and source.get('class') == 'AirCurrent' and source.get('zone') == ZONE_ID
            and source.get('zone_name') == ZONE_NAME
            and source.get('translate') == [39850., -5200., 38200.]
            and source.get('rotate') == [-90., -0., 0.] and source.get('scale') == [1., 1., 1.]
            and (source.get('wait_frame'), source.get('blow_frame'), source.get('delay_frame')) == (30, -1, 0)
            and source.get('force_calc') is False):
        return source
    return None


def parse_cell(cell):
    """The rewrite's `collision` cell: `key=value;...` (values may contain `:`, `|`, `/`)."""
    out = {}
    for item in (cell or '').split(';'):
        key, _, value = item.partition('=')
        if key:
            out[key] = value
    return out


def pose_of(row):
    return bits(row['bits_pos'], '/'), bits(row['bits_up'], '/')


def refused_state(*rows):
    return any(s in REFUSING_STATES for r in rows for s in r.get('state', '').split('/'))


class Evidence:
    """Frames on which the wind's whole traffic with Mario is proved: `frames`; why not: `rejected`."""

    def __init__(self, geometry, winds, calls, mario, rewrite, rows, preroll):
        self.frames, self.rejected, self.error = set(), {}, None
        self.gated = False  # the exported placement is the one this evidence covers
        self.identity = None  # (actor token, host class, host name) of the proved current
        self.recorded_frames = set()  # judged frames on which the probe recorded wind calls
        self.calls = {}  # frame -> {sensor: (force, answer)} the source sends
        self.overlap = {}  # frame -> the sensors of Mario that overlap the current's sphere
        try:
            self.verify(geometry, winds, calls, mario, rewrite, rows, preroll)
        except (ValueError, KeyError, TypeError, IndexError, OverflowError, struct.error, StopIteration) as error:
            self.frames, self.identity, self.calls, self.overlap = set(), None, {}, {}
            self.error = f'{type(error).__name__}: {error}'

    def verify(self, geometry, winds, calls, mario, rewrite, rows, preroll):
        placement = gate(geometry)
        self.gated = placement is not None
        if placement is None or not winds or not calls or not mario or not rewrite or not rows:
            return
        source = Source(placement)
        self.recorded_frames = {int(c['frame']) for c in calls if int(c['frame']) in rows}
        actors = {int(r['frame']): r for r in winds if r['id'] == ID}
        by_frame = {}
        for call in calls:
            by_frame.setdefault(int(call['frame']), []).append(call)
        poses = dict(preroll)
        poses.update(rows)
        mario_rows = {int(r['frame']): r for r in mario}
        self.placement_matches(source, placement, actors)
        token = {r['actor'] for r in actors.values()}
        if len(token) != 1:
            raise ValueError('current identity')
        self.identity = (token.pop(), 'AirCurrent', HOST_NAME)
        if set(actors) != set(range(min(actors), max(actors) + 1)):
            raise ValueError('actor rows are not one run of consecutive frames')
        first = min(actors)
        buffered = len(by_frame.get(first, [])) + int(actors[first]['call_dropped'])
        if buffered != len(SENSORS) * (int(actors[first]['step']) - 1):
            raise ValueError("the first sample's call count does not match its nerve step")
        for f in sorted(rows):
            why = (self.recorded_frame(source, f, actors, by_frame, poses, mario_rows)
                   or self.replayed_frame(source, f, actors, rewrite, rows))
            if why:
                self.rejected[f] = why
            else:
                self.frames.add(f)

    def refusals(self):
        """Why each recorded frame of the covered current is not proved (empty for another placement)."""
        if not self.gated:
            return {}
        why = self.error or 'no recorded wind data'
        return {f: self.rejected.get(f, why) for f in self.recorded_frames if f not in self.frames}

    @staticmethod
    def placement_matches(source, placement, actors):
        """The recorded source is the exported placement, evaluated from its pose and constant arguments."""
        for f, row in actors.items():
            if (row['zone'], row['wait_frame'], row['blow_frame'], row['delay_frame']) != (
                    ZONE_ID, '30', '-1', '0'):
                raise ValueError(f'frame {f}: recorded arguments differ from the placement')
            if (row['moves'], row['move_blow'], row['force_calc'], row['dead'], row['clipped'],
                    row['phase']) != ('0', '0', '0', '0', '0', 'Blow'):
                raise ValueError(f'frame {f}: the source is not an unclipped, immobile Blow current')
            if (bits(row['init_pos_bits'], ',') != source.translate or bits(row['pos_bits'], ',') != source.translate
                    or single(row['blow_length_bits']) != source.length
                    or bits(row['sensor_pos_bits'], ',') != source.centre
                    or single(row['sensor_radius_bits']) != source.radius):
                raise ValueError(f'frame {f}: recorded pose or sphere differ from the placement')
            if bits(row['front_bits'], ',') != source.front:
                raise ValueError(f'frame {f}: recorded front differs from the placement')
            # `quat_bits` is not compared: the sampled quaternion is one ulp off the one the pose keeper holds
            # when the helper runs (the front derived from the placement's quaternion is the recorded one).

    def recorded_frame(self, source, f, actors, by_frame, poses, mario_rows):
        """One sample of the recording against the source, the sensors and the update formula; None if proved."""
        actor = actors[f]
        step = int(actor['step'])
        if f - 1 in actors and int(actors[f - 1]['step']) + 1 != step:
            return 'recorded nerve step does not advance by one'
        if single(actor['rate_bits']) != blow_rate(step):
            return 'recorded rate is not calcNerveRate(20)'
        if actor['call_dropped'] != '0' or actor['registry_dropped'] != '0':
            return 'the probe dropped wind calls'
        before, now = poses.get(f - 1), poses[f]
        if before is None:
            return 'no pose of the previous row'
        if refused_state(before, now):
            return 'Mario is in a state that refuses or resets the wind'
        rate = blow_rate(step - 1)  # the phase of frame f-1 read the step the previous sample showed
        observed = by_frame.get(f, [])
        expected = []
        overlapping = set()
        for name, offset, radius, answers in SENSORS:
            centre = sensor_centre(pose_of(before), offset)
            if source.overlaps(centre, radius):
                overlapping.add(name)
            force = source.sent(centre, radius, rate)
            if force is not None:
                expected.append((name, force, answers))
        if [c['receiver_sensor'] for c in observed] != [e[0] for e in expected]:
            return 'recorded sensors differ from the ones the source reaches'
        accepted = []
        for call, (name, force, answers) in zip(observed, expected):
            if (call['id'], call['receiver_host'], call['sender_host']) != (ID, '箱庭マリオ', HOST_NAME):
                return 'recorded call has another sender or receiver'
            if bits(call['force_bits'], ',') != force or call['answer'] != str(int(answers)):
                return 'recorded force or answer differ from the source'
            offset = next(o for n, o, _, _ in SENSORS if n == name)
            if bits(call['receiver_pos_bits'], ',') != sensor_centre(pose_of(now), offset):
                return 'recorded sensor position differs from Mario'
            if answers:
                accepted.append(force)
        self.calls[f] = {c['receiver_sensor']: (bits(c['force_bits'], ','), c['answer']) for c in observed}
        self.overlap[f] = overlapping
        # Mario's external velocity follows the update formula on the accepted forces.
        mine, last = mario_rows.get(f), mario_rows.get(f - 1)
        if mine is None or last is None:
            return 'no recorded external velocity'
        grounded = poses[f - 1].get('ground') == '1'
        want = external_step(bits(last['external_velocity_bits'], ','), keeper_force(accepted), grounded)
        if bits(mine['external_velocity_bits'], ',') != want:
            return 'recorded external velocity differs from PlayerExternalVelocity::update'
        total = add(add(bits(mine['last_ground_inertia_bits'], ','), want), bits(mine['area_velocity_bits'], ','))
        if bits(mine['total_velocity_bits'], ',') != total:
            return 'recorded total velocity differs from (last + external) + area'
        return None

    def replayed_frame(self, source, f, actors, rewrite, rows):
        """The rewrite's row against the same source: pose, nerve, its own sensors, sends and velocity."""
        cell = parse_cell(rewrite[f]['collision']) if f in rewrite else {}
        if not cell.get('wind'):
            return 'the rewrite replayed no wind'
        name, nerve, step, power = cell['wind'].split('|')[0].split('/')
        step = int(step)
        # The rewrite's row holds the step the previous frame's sensor phase used: one below the sampled one.
        if (name, nerve, step) != (ID, 'Blow', int(actors[f]['step']) - 1):
            return f"the rewrite's source phase ({nerve} {step}) differs from the recording's"
        if power != hex_bits([blow_rate(step)]):
            return "the rewrite's blow rate is not calcNerveRate(20)"
        if cell['windpose'] != f'{ID}:{hex_bits(source.translate)}:{hex_bits(source.quat)}':
            return "the rewrite's source pose differs from the placement"
        rate = blow_rate(step)
        centres = {}
        for item in cell['windsensors'].split('|'):
            sensor, _, text = item.partition(':')
            centres[sensor] = bits(text, '/')
        sends = []
        for sensor, offset, radius, _ in NATIVE_SENSORS:
            force = source.sent(centres[sensor], radius, rate)
            if force is not None:
                sends.append(f'{ID}:{sensor}:{hex_bits(force)}')
        if cell['windsent'].split('|') != (sends or ['']):
            return 'the rewrite\'s sends differ from what the source predicts from its sensors'
        forces = [bits(s.split(':')[2], '/') for s in sends]
        if bits(cell['windforce'], '/') != keeper_force(forces):
            return 'the rewrite\'s keeper force differs from min + max of its sends'
        previous = parse_cell(rewrite[f - 1]['collision']) if f - 1 in rewrite else {}
        if previous.get('windvelocity'):
            grounded = cell['windground'] == '1'
            if grounded != (rewrite[f - 1].get('ground') == '1'):
                return "the rewrite's ground flag is not the previous update's"
            want = external_step(bits(previous['windvelocity'], '/'), bits(cell['windforce'], '/'), grounded)
            if bits(cell['windvelocity'], '/') != want:
                return "the rewrite's external velocity differs from PlayerExternalVelocity::update"
        return None

    def contact(self, frame, contact):
        """Whether one typed contact row of Mario with the current is covered by the proved frame."""
        if frame not in self.frames or not self.identity or contact['who'] != 'player':
            return False
        if (contact['actor'], contact['host_class'], contact['host']) != self.identity:
            return False
        if (contact['other'], contact['other_type']) != ('Attack', '10'):
            return False
        sensors = {n: (answers, radius) for n, _, radius, answers in SENSORS}
        if contact['self'] not in sensors:
            return False  # Eye overlaps are the look-at rule's
        if contact['kind'] == 'a':
            return contact['self'] in self.overlap.get(frame, ())
        if contact['kind'] != 'r' or contact['message'] != MESSAGE:
            return False
        call = self.calls.get(frame, {}).get(contact['self'])
        return call is not None and contact['answer'] == call[1]


def self_check():
    assert gate(None) is None and gate({'stage': STAGE, 'scenario': 1}) is None
    assert not Evidence(None, None, None, None, None, None, None).frames
    placement = dict(id=ID, **{'class': 'AirCurrent'}, zone=ZONE_ID, zone_name=ZONE_NAME, translate=[39850., -5200., 38200.],
                     rotate=[-90., -0., 0.], scale=[1., 1., 1.], wait_frame=30, blow_frame=-1, delay_frame=0,
                     force_calc=False)
    geometry = dict(stage=STAGE, scenario=1, air_currents=[placement])
    assert gate(geometry) is placement
    for key, value in (('zone_name', 'OtherZone'), ('id', 'obj1'), ('wait_frame', 31), ('scale', [1., 1., 2.]),
                       ('rotate', [0., 0., 0.]), ('force_calc', True), ('class', 'Other')):
        assert gate(dict(geometry, air_currents=[dict(placement, **{key: value})])) is None
    assert gate(dict(geometry, scenario=2)) is None and gate(dict(geometry, air_currents=[placement, placement])) is None
    source = Source(placement)
    # The placement rotates -90 degrees about X: the front is +Y and the sphere stands on the origin end.
    assert source.front[1] > 0.99999 and abs(source.front[0]) < 1e-6 and source.length == 2000.
    assert abs(source.centre[1] - (-5200. + 1100.)) < 1. and source.radius == 1100.
    up = lambda height: [39850., -5200. + height, 38200.]
    # On the axis the force is the base 12 times the rate; the near-origin cone and the last 400 of the length
    # taper it, and past the length (plus the sensor's radius) nothing is sent.
    assert abs(source.force(up(500.), 1.)[1] - 12.) < 1e-4 and source.force(up(500.), 0.5)[1] < 6.001
    assert abs(source.force(up(1800.), 1.)[1] - 6.) < 1e-3 and source.force(up(2100.), 1.) == [0., 0., 0.]
    off_axis = lambda height, side: [39850. + side, -5200. + height, 38200.]
    assert source.force(off_axis(50., 150.), 1.) == [0., 0., 0.]  # outside the cone near the origin
    assert 5.9 < source.force(off_axis(1000., 325.), 1.)[1] < 6.1  # half-way through the transverse taper
    assert source.force(off_axis(1000., 450.), 1.) == [0., 0., 0.]
    assert source.sent(up(500.), 40., 0.) is None and source.sent(up(5000.), 40., 1.) is None
    assert source.sent(up(500.), 40., 1.) is not None
    assert blow_rate(0) == 0. and blow_rate(10) == 0.5 and blow_rate(20) == 1. and blow_rate(99) == 1.
    # The keeper keeps the extrema of the frame's forces and the velocity follows v * drag + force * 0.275.
    assert keeper_force([[0., 1., 0.], [0., 3., 0.], [0., -2., 0.]]) == [0., 1., 0.]
    assert external_step([0., 10., 0.], [0., 2., 0.], False) == [0., f32(f32(10 * f32(.955)) + f32(2 * f32(.275))), 0.]
    assert external_step([0., 0.05, 0.], [0., 0., 0.], True) == [0., 0., 0.]
    _check_synthetic(placement, geometry, source)


def _check_synthetic(placement, geometry, source):
    """A consistent synthetic recording and replay is proved; each corrupted field is not."""
    steps = range(-2, 4)
    height = {f: -5100. + 30. * f for f in steps}
    pos = lambda f: [39850., height[f], 38200.]
    rows, preroll, winds, calls, mario, rewrite = {}, {}, [], [], [], {}
    zero = ','.join(['00000000'] * 3)
    velocity = {-2: [0., 0., 0.]}
    for f in steps:
        row = dict(frame=str(f), bits_pos='/'.join('0x' + h for h in hex_bits(pos(f)).split('/')),
                   bits_up='/'.join('0x' + h for h in hex_bits([0., 1., 0.]).split('/')), ground='0', state='Fall/Fall')
        (rows if f >= 0 else preroll)[f] = row
    for f in steps:
        step = 8 + f  # a ramp still running: the offset between sampled step and phase step shows
        rate = blow_rate(step - 1)  # what the sensor phase of frame f-1 used
        winds.append(dict(frame=str(f), id=ID, zone=ZONE_ID, actor='7', wait_frame='30', blow_frame='-1', delay_frame='0',
                          moves='0', move_blow='0', force_calc='0', dead='0', clipped='0', phase='Blow', step=str(step),
                          rate_bits=hex_bits([blow_rate(step)]), init_pos_bits=hex_bits(source.translate).replace('/', ','),
                          pos_bits=hex_bits(source.translate).replace('/', ','),
                          blow_length_bits=hex_bits([source.length]),
                          sensor_pos_bits=hex_bits(source.centre).replace('/', ','),
                          sensor_radius_bits=hex_bits([source.radius]),
                          front_bits=hex_bits(source.front).replace('/', ','),
                          quat_bits=hex_bits(source.quat).replace('/', ','),
                          call_dropped='5' if f == -2 else '0', registry_dropped='0'))
        if f == -2:  # the first sample's buffer: 5 sensors x (step 6 - 1) phases, 5 of them dropped
            calls.extend(dict(frame='-2', id=ID) for _ in range(20))
        if f - 1 not in height:
            mario.append(dict(frame=str(f), external_velocity_bits=zero, last_ground_inertia_bits=zero,
                              area_velocity_bits=zero, total_velocity_bits=zero))
            continue
        accepted = []
        for name, offset, radius, answers in SENSORS:
            centre = sensor_centre(pose_of(rows.get(f - 1) or preroll[f - 1]), offset)
            force = source.sent(centre, radius, rate)
            if force is None:
                continue
            calls.append(dict(frame=str(f), id=ID, receiver_host='箱庭マリオ', sender_host=HOST_NAME, receiver_sensor=name,
                              force_bits=hex_bits(force).replace('/', ','), answer=str(int(answers)),
                              receiver_pos_bits=hex_bits(sensor_centre(pose_of(rows.get(f) or preroll[f]), offset)).replace('/', ',')))
            if answers:
                accepted.append(force)
        velocity[f] = external_step(velocity[f - 1], keeper_force(accepted), False)
        mario.append(dict(frame=str(f), external_velocity_bits=hex_bits(velocity[f]).replace('/', ','),
                          last_ground_inertia_bits=zero, area_velocity_bits=zero,
                          total_velocity_bits=hex_bits(add(add([0.] * 3, velocity[f]), [0.] * 3)).replace('/', ',')))
        if f >= 0:
            centres = {n: sensor_centre(pose_of(rows.get(f - 1) or preroll[f - 1]), o) for n, o, _, _ in NATIVE_SENSORS}
            sends = [(n, source.sent(centres[n], r, rate)) for n, _, r, _ in NATIVE_SENSORS]
            sends = [(n, s) for n, s in sends if s is not None]
            rewrite[f] = dict(collision=(
                f'wind={ID}/Blow/{step - 1}/{hex_bits([rate])};windpose={ID}:{hex_bits(source.translate)}:{hex_bits(source.quat)};'
                f'windsensors={"|".join(n + ":" + hex_bits(c) for n, c in centres.items())};'
                f'windsent={"|".join(f"{ID}:{n}:{hex_bits(s)}" for n, s in sends)};'
                f'windforce={hex_bits(keeper_force([s for _, s in sends]))};windvelocity={hex_bits(velocity[f])};windground=0'))
    good = Evidence(geometry, winds, calls, mario, rewrite, rows, preroll)
    assert good.error is None and good.frames == set(rows) and good.identity == ('7', 'AirCurrent', HOST_NAME), good.error
    contact = dict(who='player', actor='7', host_class='AirCurrent', host=HOST_NAME, other='Attack', other_type='10',
                   self='Foot', self_type='3', kind='r', message=MESSAGE, answer='1')
    assert good.contact(0, contact) and good.contact(0, dict(contact, kind='a', message='', answer=''))
    # A message or answer the source does not give, another sensor pair, another actor or a frame outside: refused.
    for wrong in (dict(answer='0'), dict(message='rs::SensorMsgHit'), dict(other='Body'), dict(actor='8'),
                  dict(self='Carry'), dict(self='Eye'), dict(kind='p'), dict(host_class='Coin')):
        assert not good.contact(0, dict(contact, **wrong)), wrong
    assert not good.contact(99, contact)
    attack = dict(contact, self='HipDropKnockDown', answer='0')
    assert good.contact(0, attack) and not good.contact(0, dict(attack, answer='1'))

    def broken(mutate):
        copy = [dict(r) for r in winds], [dict(r) for r in calls], [dict(r) for r in mario], {k: dict(v) for k, v in rewrite.items()}
        mutate(*copy)
        return Evidence(geometry, copy[0], copy[1], copy[2], copy[3], rows, preroll)

    flip = lambda cell: cell[:-1] + ('0' if cell[-1] != '0' else '1')
    at_zero = next(i for i, c in enumerate(calls) if c['frame'] == '0')
    assert broken(lambda w, c, m, r: c[at_zero].update(force_bits=flip(c[at_zero]['force_bits']))).frames != good.frames
    assert broken(lambda w, c, m, r: c[at_zero].update(answer='0' if c[at_zero]['answer'] == '1' else '1')).frames != good.frames
    assert broken(lambda w, c, m, r: c.pop(at_zero)).frames != good.frames
    assert broken(lambda w, c, m, r: m[-1].update(external_velocity_bits=flip(m[-1]['external_velocity_bits']))).frames != good.frames
    assert broken(lambda w, c, m, r: w[-1].update(step='99')).frames != good.frames
    # The counters of the first sample must agree with its nerve step.
    assert broken(lambda w, c, m, r: w[0].update(call_dropped='6')).frames == set()
    assert broken(lambda w, c, m, r: w[0].update(clipped='1')).frames == set()
    assert broken(lambda w, c, m, r: w[0].update(wait_frame='31')).frames == set()
    # The rewrite's own phase, pose, sends and velocity are checked as well.
    assert 0 not in broken(lambda w, c, m, r: r[0].update(collision=r[0]['collision'].replace('/Blow/7/', '/Blow/8/'))).frames
    assert 0 not in broken(lambda w, c, m, r: r[0].update(collision=r[0]['collision'].replace(f'windsent={ID}', f'windsent=x{ID}'))).frames
    assert 1 not in broken(lambda w, c, m, r: r[1].update(collision=r[1]['collision'].replace('windground=0', 'windground=1'))).frames
    assert broken(lambda w, c, m, r: r.pop(0)).frames == set(rows) - {0}


if __name__ == '__main__':
    self_check()
