"""Placement-bound ConveyerMapParts source and exact exported collision identity evidence.

Independent of the Rust port: the whole timeline of the recorded host, its steps and their collision
matrices is re-simulated from the placement (al::ConveyerMapParts::exeMove, ConveyerStep::setTransByCoord,
ConveyerKeyKeeper::calcPosAndQuat, all in single precision) and only the offset of the first recorded
sample comes from the recording. Observed fields validate the implementation; no field drives replay.
"""
import math
import struct
from key_move_parts import f32, bits

# The conveyors whose exported collision and recorded timeline this evidence can vouch for, bound by
# stage, scenario and placement id; every argument is what the placement holds and the rewrite reproduces.
PLACEMENTS = {
    ('SandWorldPyramid001Stage', 1, 'obj243'): dict(
        model='SandWorldPyramid001ConveyerParts000', speed=2.0, interval=800.0, steps=5, start_rate=0.0,
        keys=[[2350, 1450, -14100], [2350, 1450, -17700]], total=3600.0, triangles=32),
}
STEP_HOST = 'コンベア足場'


def floats(cell, count, sep=','):
    values = [struct.unpack('>f', bytes.fromhex(x))[0] for x in cell.split(sep)]
    if len(values) != count or not all(math.isfinite(v) for v in values):
        raise ValueError('conveyor bits')
    return values


def wrap(value, maximum):
    """al::wrapValue: fmodf(value + max, max) + 0."""
    return f32(math.fmod(f32(value + maximum), maximum) + 0.0)


class Source:
    """ConveyerMapParts and its ConveyerKeyKeeper for a straight, unrotated, linear key chain along +Z."""

    def __init__(self, placement, offset):
        self.p = placement
        keys = [[f32(c) for c in k] for k in placement['keys']]
        self.trans = keys[0]
        self.direction = [0., 0., 1.]
        self.distance = []
        self.vertical = []
        for key in keys:
            relative = [f32(a - b) for a, b in zip(key, self.trans)]
            dot = f32(f32(f32(relative[0] * self.direction[0]) + f32(relative[1] * self.direction[1])) + f32(relative[2] * self.direction[2]))
            self.distance.append(dot)
            # al::verticalizeVec: setScaleAdd(-dot, vertical, vec)
            self.vertical.append([f32(f32(f32(-dot) * d) + r) for d, r in zip(self.direction, relative)])
        self.cumulative = [0.]
        for a, b in zip(self.distance, self.distance[1:]):
            self.cumulative.append(f32(self.cumulative[-1] + abs(f32(b - a))))
        self.total = self.cumulative[-1]
        self.interval = f32(max(placement['interval'], 10.))
        self.count = int(f32(self.total / self.interval)) + 1
        self.max_coord = f32(self.interval * self.count)
        self.speed = f32(placement['speed'])
        self.offset = offset
        self.ticks = 0

    def tick(self):
        self.offset = wrap(f32(self.offset + f32(1.0 * self.speed)), self.max_coord)
        self.ticks += 1

    def coord(self, offset, index):
        return wrap(f32(f32(self.interval * index) + offset), self.max_coord)

    def position(self, coord):
        """ConveyerKeyKeeper::calcPosAndQuat (position)."""
        if coord <= 0:
            return list(self.trans)
        if coord >= self.total:
            last = len(self.distance) - 1
            return [f32(f32(f32(v + f32(d * self.distance[last])) + t)) for v, d, t in zip(self.vertical[last], self.direction, self.trans)]
        index = next((i for i, c in enumerate(self.cumulative) if c > coord), 0)
        index = max(index, 1)
        span = f32(self.cumulative[index] - self.cumulative[index - 1])
        t = 0. if abs(span) < .001 else f32(f32(coord - self.cumulative[index - 1]) / span)
        vertical = [f32(a + f32(f32(b - a) * t)) for a, b in zip(self.vertical[index - 1], self.vertical[index])]
        clamped = min(max(t, 0.), 1.)
        distance = f32(f32(self.distance[index - 1] * f32(1. - clamped)) + f32(clamped * self.distance[index]))
        return [f32(f32(f32(d * distance) + o) + v) for d, o, v in zip(self.direction, self.trans, vertical)]

    def step(self, offset, index):
        coord = self.coord(offset, index)
        return coord, self.position(coord), coord <= self.total

    def previous_offset(self, offset):
        return wrap(f32(offset - self.speed), self.max_coord)


def matrix(position):
    return [1., 0., 0., position[0], 0., 1., 0., position[1], 0., 0., 1., position[2]]


class Evidence:
    def __init__(self, geometry, hosts, steps, messages, mario, rolling, rewrite, rows, preroll, start):
        self.surfaces, self.rides = set(), set()
        self.id = self.host = None
        try:
            self._verify(geometry, hosts, steps, messages, mario, rolling, rewrite, rows, preroll, start)
        except (KeyError, TypeError, ValueError, IndexError, StopIteration, OverflowError, ZeroDivisionError, struct.error):
            self.surfaces, self.rides = set(), set()

    def _verify(self, geometry, hosts, steps, messages, mario, rolling, rewrite, rows, preroll, start):
        if not geometry or not hosts or not steps or messages is None or not mario or not rolling or not start:
            return
        from environment_rule import key_values, parse_hit_hosts
        placements = geometry.get('conveyor_parts', [])
        if len(placements) != 1:
            return
        p = placements[0]
        expected = PLACEMENTS.get((geometry.get('stage'), geometry.get('scenario'), p.get('id')))
        if not expected:
            return
        exported = {'id': p['id'], 'class': 'ConveyerMapParts', 'model': expected['model'], 'host': STEP_HOST,
                    'speed': expected['speed'], 'interval': expected['interval'], 'start_rate': expected['start_rate'],
                    'steps': expected['steps'], 'keys': expected['keys'], 'total': expected['total']}
        if p != exported:
            return
        self.id, self.host = p['id'], p['host']
        objects = [o for o in geometry['objects'] if o['id'].startswith(p['id'] + '/')]
        if len(objects) != expected['steps']:
            return
        raw_by_step = {}
        for i in range(expected['steps']):
            index = next(n for n, o in enumerate(geometry['objects']) if o['id'] == f"{p['id']}/{i}")
            obj = geometry['objects'][index]
            if obj['model'] != p['model'] or obj['static'] is not False or obj['rotate'] != [0, 0, 0] or obj['scale'] != [1, 1, 1]:
                return
            tris = [t for t in geometry['triangles'] if t[13] == index]
            if len(tris) != expected['triangles']:
                return
            raw_by_step[i] = {t[17] for t in tris if t[16] == 0}
        if not geometry['models'][p['model'] + '#0']['prisms']:
            return
        all_rows = dict(preroll)
        all_rows.update(rows)
        host_rows = {int(r['frame']): r for r in hosts if r['id'] == p['id']}
        step_rows = {}
        for r in steps:
            if r['host'] == p['id']:
                step_rows.setdefault(int(r['frame']), {})[int(r['index'])] = r
        c = {int(r['frame']): r for r in mario}
        contacts = {int(r['frame']): r for r in rolling}
        frames = sorted(all_rows)
        if set(host_rows) != set(all_rows) or set(step_rows) != set(all_rows) or set(c) != set(all_rows) or set(contacts) != set(all_rows):
            return
        if frames[0] != -30:
            return
        # The first sample's message buffer also holds what the host received while the probe placed the
        # player (before the first replayed update), so only later samples must have lost none. The host acts on
        # one message, isMsgFloorTouch (ConveyerMapParts::receiveMsg 0x912b94), which only feeds the ride
        # counters that exeMove 0x912cb4 reads when IsRideOnlyMove is set (ride_only is required to be 0).
        # Messages a floor-standing player sends the host: PlayerTouch, PlayerFloorTouch, AskSafetyPoint, PlayerJumpTakeOffFloor.
        if any(r['answer'] not in ('0', '1') or r['message_vt'] not in ('1de9b18', '1de93e8', '1deb878', '1d9dad8') for r in messages if r['id'] == p['id']):
            return
        first = host_rows[frames[0]]
        source = Source(p, floats(first['offset_bits'], 1)[0])
        second = host_rows[frames[1]]
        # A first sample taken after the host update of its own frame repeats its offset in the next one.
        held = second['offset_bits'] == first['offset_bits']
        last = source.previous_offset(source.offset)
        before = source.previous_offset(last)
        # collision position history per step: actor positions of the last and the second last update
        history = {i: [source.step(before, i)[1], source.step(last, i)[1]] for i in range(source.count)}
        actor = {i: source.step(source.offset, i)[1] for i in range(source.count)}
        executions = {i: 1 for i in range(source.count)}
        coords = {i: source.step(source.offset, i)[0] for i in range(source.count)}
        valid = {i: source.step(source.offset, i)[2] for i in range(source.count)}
        generation = None
        pointers = {}
        surfaces, rides = set(), set()
        for k, f in enumerate(frames):
            if k > 0 and not (k == 1 and held):
                source.tick()
                for i in range(source.count):
                    coord, position, exists = source.step(source.offset, i)
                    # the collision part is synchronised with its actor only while it is valid
                    if valid[i]:
                        history[i] = [history[i][1], actor[i]]
                        executions[i] += 1
                    # A wrap (setTransByCoord 0x9130bc, newCoord < mCurrentCoord going forwards) calls
                    # resetPosition -> resetAllCollisionMtx 0x8d7ee8: both collision matrices become the new pose.
                    if coord < coords[i]:
                        history[i] = [position, position]
                    coords[i] = coord
                    actor[i] = position
                    valid[i] = exists
            r = host_rows[f]
            if generation is None:
                generation = r['generation']
            if (r['generation'] != generation or r['phase'] != 'Move' or r['n_vt'] != '1dedf48'
                    or any(r[key] != '0' for key in ('dead', 'clipped', 'SwitchStart_valid', 'ride_only', 'registry_dropped'))
                    or (k > 0 and r['message_dropped'] != '0')
                    or floats(r['offset_bits'], 1)[0] != source.offset
                    or floats(r['speed_bits'], 1)[0] != source.speed or floats(r['interval_bits'], 1)[0] != source.interval
                    or floats(r['max_coord_bits'], 1)[0] != source.max_coord or floats(r['key_total_bits'], 1)[0] != source.total
                    or int(r['steps']) != source.count or int(r['keys']) != len(expected['keys'])
                    or floats(r['key_quat_bits'], 4) != [0., 0., 0., 1.] or floats(r['key_trans_bits'], 3) != source.trans
                    or floats(r['key_dir_bits'], 3) != source.direction or floats(r['init_quat_bits'], 4) != [0., 0., 0., 1.]
                    or r['zone']):
                return
            if f >= 0 and f in rewrite:
                diagnostic = '|'.join([f"{p['id']}/{i}/Step/{executions[i]}" for i in range(source.count)] + [f"{p['id']}/Move/{bits(source.offset)}"])
                if key_values(rewrite[f]['collision']).get('rotors') != diagnostic:
                    return
            for i in range(source.count):
                s = step_rows[f][i]
                coord, position, exists = source.step(source.offset, i)
                if (s['generation'] != generation or s['dead'] != '0'
                        or floats(s['current_coord_bits'], 1)[0] != coord or floats(s['max_coord_bits'], 1)[0] != source.max_coord
                        or s['exists'] != ('1' if exists else '0')
                        or floats(s['pos_bits'], 3) != position or floats(s['quat_bits'], 4) != [0., 0., 0., 1.]
                        or floats(s['matrix_bits'], 12) != matrix(history[i][1]) or floats(s['previous_matrix_bits'], 12) != matrix(history[i][0])
                        or s['collision_user_valid'] != ('1' if exists else '0') or s['collision_system_valid'] != '1'):
                    return
                if i in pointers and pointers[i] != s['part']:
                    return
                pointers[i] = s['part']
            row, hit, side = all_rows[f], contacts[f], c[f]
            if any(side[key] != '0' for key in ('pressed', 'pressed_ground', 'pressed_wall', 'gate_true')):
                return
            for kind in ('ground', 'wall'):
                part = hit.get(kind + '_part')
                if not part:
                    continue
                index = next((i for i, v in pointers.items() if v == part), None)
                if index is None or int(hit[kind + '_tri']) not in raw_by_step[index]:
                    return
                if kind == 'ground':
                    if (side['ground_part'] != part or floats(side['ground_matrix_bits'], 12) != matrix(history[index][1])
                            or floats(side['ground_previous_matrix_bits'], 12) != matrix(history[index][0])):
                        return
                host = parse_hit_hosts(row.get('hit_hosts')).get(kind)
                if f >= 0:
                    base = tuple(round(f32(f32(x) - f32(o)), 6) for x, o in zip(history[index][1], start))
                    if not host or host['host'] != STEP_HOST or host['base'] != base or host['sensor'] != 'Collision' or host['sensor_type'] != '15':
                        return
                    surfaces.add((f, kind, host['host'], host['base']))
                    if kind == 'ground':
                        rides.add(f)
        if len(set(pointers.values())) == source.count:
            self.surfaces, self.rides = surfaces, rides

    def surface(self, f, kind, host):
        return (f, kind, host.get('host'), host.get('base')) in self.surfaces

    def riding(self, f):
        return f in self.rides
