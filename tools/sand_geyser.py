"""Independent verification of exported SandGeyser sources and their sampled phase.

All actor evidence is separate from Mario's trace. Scalar operations follow 1.0.0.
"""
import struct
import math


def f32(x):
    return struct.unpack('<f', struct.pack('<f', x))[0]


def bits(x):
    return struct.pack('>f', x).hex()


PHASES = ['Wait', 'UpStart', 'UpLoop', 'UpWait', 'UpEnd', 'UpEndWait']


class Source:
    def __init__(self, placement):
        self.placement = placement
        self.phase, self.step, self.top = 0, 0, 0.
        self.radius, self.valid = 450., True
        self.centre = list(placement['position'])
        self.animation, self.animation_frame, self.started = 0, 0, False

    def tick(self):
        p = self.placement
        if self.started:
            self.animation_frame += 1
            if self.animation != 4:
                self.animation_frame %= {1: 15, 2: 20}.get(self.animation, 30)
        if self.step == 0 and self.phase != 3:
            self.animation, self.animation_frame, self.started = (0 if self.phase == 5 else self.phase), 0, True
        phase, step = self.phase, self.step
        next_phase = None
        if phase == 0 and step == p['up_start']:
            next_phase = 1
        elif phase == 1:
            if step == 0:
                self.top = 0.
            if step == 180:
                next_phase = 2
        elif phase == 2:
            self.valid = True
            self.top = f32(f32(p['target_y'] - p['position'][1]) * f32(step / p['up_time']))
            if step <= p['up_time']:
                self.radius = max(f32(f32(self.top * .5) + 200.), 175.)
                self.centre[1] = f32(f32(p['position'][1] - 200.) + self.radius)
            else:
                next_phase = 3
        elif phase == 3 and step == 300:
            next_phase = 4
        elif phase == 4:
            self.valid = False
            if step == 30:
                next_phase = 5
        elif phase == 5 and step == p['end_wait']:
            next_phase = 1
        if next_phase is None:
            self.step += 1
        else:
            self.phase, self.step = next_phase, 0

    def base(self, curves):
        position = list(self.placement['position'])
        if self.animation == 1:
            def sample(keys):
                at = max(0, sum(key[0] <= self.animation_frame for key in keys) - 1)
                key = keys[at]
                if at == len(keys) - 1 or self.animation_frame <= keys[0][0]:
                    return key[1]
                t = f32(f32(self.animation_frame - key[0]) / f32(keys[at + 1][0] - key[0]))
                return f32(f32(f32(f32(f32(key[4] * t) + key[3]) * t) + key[2]) * t + key[1])
            offset = [sample(keys) for keys in curves['top_translation']]
            position = [f32(position[0] + offset[0]),
                        f32(f32(position[1] + curves['parent_y']) + offset[1]),
                        f32(position[2] + offset[2])]
        position[1] = f32(position[1] + self.top)
        return position

    def diagnostic(self):
        return f"{self.placement['id']}/{self.phase}/{self.step}/{bits(self.top)}/{bits(self.radius)}/{int(self.valid)}"


class Evidence:
    def __init__(self, geometry, actor_rows, rewrite, seed, messages=None):
        self.sources, self.tokens = {}, {}
        self.frames, self.bases, self.messages = {}, {}, set()
        try:
            self._verify(geometry, actor_rows, rewrite, seed, messages)
        except (KeyError, TypeError, ValueError, IndexError, OverflowError, ZeroDivisionError):
            # Incomplete or unsupported evidence proves no source; normal refusal applies.
            self.sources, self.tokens = {}, {}
            self.frames, self.bases, self.messages = {}, {}, set()

    def _verify(self, geometry, actor_rows, rewrite, seed, messages):
        if not geometry or not actor_rows or not seed:
            return
        placements = geometry.get('sand_geysers', [])
        if not placements or (seed.get('stage'), seed.get('scenario')) != (geometry['stage'], geometry['scenario']):
            return
        if (len({p['id'] for p in placements}) != len(placements)
                or any(p['up_time'] <= 0 or p['up_start'] < 0 or p['end_wait'] < 0
                       or not all(math.isfinite(v) for v in p['position'] + [p['target_y']]) for p in placements)):
            return
        sources = {p['id']: Source(p) for p in placements}
        by_frame = {}
        for row in actor_rows:
            by_frame.setdefault(int(row['frame']), []).append(row)
        first = min(by_frame)
        if seed.get('frame') != first or first >= 0 or seed.get('sources') != by_frame[first]:
            return
        tokens, frames, bases = {}, {}, {}
        generations, expected_end = {}, {}
        current_bases = {identity: list(source.placement["position"]) for identity, source in sources.items()}
        for frame in range(first, max(by_frame) + 1):
            rows = by_frame.get(frame, [])
            if {r['id'] for r in rows} != set(sources) or len(rows) != len(sources):
                return
            for row in rows:
                source = sources[row['id']]
                p = source.placement
                part = next((o for o in geometry['objects'] if o['id'] == p['id']), {})
                if (part.get('model') != 'SandGeyser' or part.get('translate') != p['position']
                        or part.get('rotate') != [0, 0, 0] or part.get('scale') != [1, 1, 1]
                        or not geometry.get('models', {}).get('SandGeyser#0', {}).get('prisms')
                        or any(row[k] != '0' for k in ('dead', 'clipped', 'registry_dropped'))
                        or [float(row['init_' + a]) for a in 'xyz'] != p['position']
                        or row['init_bits'] != ','.join(bits(v) for v in p['position'])
                        or row['pos_bits'] != row['init_bits']
                        or row['scale_bits'] != '3f800000,3f800000,3f800000'
                        or row['quat_bits'] != '00000000,00000000,00000000,3f800000'
                        or row['sensor'] != '1'
                        or row['n'] != ['1d2a648', '1d2a688', '1d2a6c8', '1d2a708', '1d2a748', '1d2a788'][source.phase]
                        or row['phase'] != PHASES[source.phase] or int(row['step']) != source.step
                        or row['top_local_bits'] != bits(source.top)
                        or row['radius_bits'] != bits(source.radius)
                        or [bits(float(row['centre_' + a])) for a in 'xyz'] != [bits(v) for v in source.centre]
                        or row['valid'] != str(int(source.valid)) or row['system'] != '1'):
                    return
                generation = generations.setdefault(p['id'], row['generation'])
                if generation != row['generation'] or row['action_end'] != expected_end.get(p['id'], '-1'):
                    return
                if row['actor'] != '0':
                    if row['actor'] in tokens and tokens[row['actor']] != p['id']:
                        return
                    tokens[row['actor']] = p['id']
                bases[frame, p["id"]] = current_bases[p["id"]]
                frames[frame, p['id']] = (source.top, source.radius, list(source.centre), source.valid)
            if frame >= 0:
                diagnostic = '|'.join(source.diagnostic() for source in sources.values())
                collision = rewrite.get(frame, {}).get('collision', '')
                if f'geysers={diagnostic}' not in collision:
                    return
            for identity, source in sources.items():
                expected_end[identity] = str(int(source.step == 30)) if source.phase == 4 else '-1'
                current_bases[identity] = source.base(geometry["sand_geyser_curves"])
                source.tick()
        for row in messages or []:
            p = next((p for p in placements if p['id'] == row['id']), None)
            if (not p or tokens.get(row['actor']) != p['id'] or row['dropped'] != '0'
                    or row['target_bits'] != bits(p['target_y'])
                    or row['speed_bits'] != bits(f32(f32(p['target_y'] - p['position'][1]) / p['up_time']))):
                return
        self.messages = {(int(r['frame']), r['actor'], r['k'], r['answer']) for r in messages or []}
        self.sources, self.tokens, self.frames, self.bases = {p['id']: p for p in placements}, tokens, frames, bases

    def floor(self, frame, host, start):
        if not start or not host or host.get('base') is None:
            return False
        for identity, p in self.sources.items():
            state = self.frames.get((frame, identity))
            if state and state[3] and host['host'] == p['host'] and host['sensor'] == 'Body':
                base = [f32(a-b) for a,b in zip(self.bases[frame, identity], start)]
                # Probe prints six decimals; compare in its own decimal representation.
                if tuple(round(v, 6) for v in base) == host['base']:
                    return True
        return False

    def carries(self, frame, host, start, rows, tolerance):
        """The exported upright Top's translation must explain the horizontal residual.

        The tolerance is the environment rule's existing grounded residual threshold.
        No additional contact margin is introduced.
        """
        if not self.floor(frame, host, start) or frame - 1 not in rows:
            return False
        for identity in self.sources:
            before = self.bases.get((frame - 1, identity))
            after = self.bases.get((frame, identity))
            if before is None or after is None:
                continue
            base = tuple(round(f32(v - o), 6) for v, o in zip(after, start))
            if base != host['base']:
                continue
            row, prev = rows[frame], rows[frame - 1]
            residual = [float(row[p]) - float(prev[p]) - float(row[v]) - f32(after[a] - before[a])
                        for a, p, v in [(0, 'px', 'vx'), (2, 'pz', 'vz')]]
            if math.hypot(*residual) <= tolerance:
                return True
        return False

    def contact(self, frame, contact, rows, preroll, start):
        identity = self.tokens.get(contact['actor'])
        p = self.sources.get(identity)
        if (not p or not start or contact['who'] != 'player' or contact['host_class'] != 'SandGeyser'
                or contact['host'] != p['host'] or contact['other'] != 'Raise' or contact['other_type'] != '12'):
            return False
        own = {('Foot', '3'): (40., 40.), ('Body', '1'): (75., 50.), ('Head', '1'): (110., 40.),
               ('Carry', '2'): (100., 130.), ('SpinAttack', '2'): (100., 130.),
               ('HipDropKnockDown', '2'): (0., 150.), ('Eye', '5'): (80., 800.)}.get((contact['self'],contact['self_type']))
        state = self.frames.get((frame-1, identity))
        row = rows.get(frame-1, preroll.get(frame-1, {}))
        if own is None or not state or not state[3] or not row:
            return False
        feet = [f32(number + origin) for number, origin in zip([float(row[k]) for k in ('px','py','pz')],start)]
        feet[1] = f32(feet[1] + own[0])
        delta = [f32(a-b) for a,b in zip(feet,state[2])]
        squared = f32(f32(f32(delta[0]*delta[0])+f32(delta[1]*delta[1]))+f32(delta[2]*delta[2]))
        radius = f32(own[1]+state[1])
        if squared >= f32(radius*radius):
            return False
        if contact['kind'] == 'a':
            return not contact['message'] and not contact['answer']
        horizontal = f32(f32(delta[0]*delta[0])+f32(delta[2]*delta[2]))
        cylinder = f32(175.+own[1])
        bottom = f32(p['position'][1]-200.)
        top = f32(bottom+f32(state[0]+200.))
        dead = 'Dead' in rows[frame].get('state','').split('/')
        return ((frame,contact['actor'],contact['kind'],contact['answer']) in self.messages
                and horizontal <= f32(cylinder*cylinder) and f32(feet[1]+own[1]) >= bottom
                and f32(feet[1]-own[1]) <= top
                and (contact['kind'] != 'r' or contact['answer'] == str(int(not dead)))
                and contact['kind'] in ('r', 'p') and contact['message'] == 'rs::SensorMsgSandGeyserRaise'
                and contact['self_type'] in ('1', '3') and contact['answer'] in ('0', '1')
                and (contact['kind'] != 'p' or contact['answer'] == '0'))


def self_check():
    """Exercise the evidence boundary without installed game data or saved player outputs."""
    from copy import deepcopy

    p = dict(id='source', host='Geyser', position=[0., -100., 0.], target_y=100.,
             up_start=2, up_time=2, end_wait=2)
    geometry = dict(stage='Test', scenario=1, sand_geysers=[p],
                    objects=[dict(id='source', model='SandGeyser', translate=p['position'],
                                  rotate=[0, 0, 0], scale=[1, 1, 1])],
                    models={'SandGeyser#0': dict(prisms=[[1]])},
                    sand_geyser_curves=dict(parent_y=0., top_translation=[[[0, 0, 0, 0, 0]]] * 3))
    source, actors, rewrite = Source(p), [], {}
    end = '-1'
    for frame in range(-2, 900):
        position = ','.join(bits(v) for v in p['position'])
        actors.append(dict(frame=str(frame), id='source', actor='1', generation='1',
                           dead='0', clipped='0', registry_dropped='0', init_x='0', init_y='-100', init_z='0',
                           init_bits=position, pos_bits=position, scale_bits='3f800000,3f800000,3f800000',
                           quat_bits='00000000,00000000,00000000,3f800000', sensor='1',
                           n=['1d2a648', '1d2a688', '1d2a6c8', '1d2a708', '1d2a748', '1d2a788'][source.phase],
                           phase=PHASES[source.phase], step=str(source.step), top_local_bits=bits(source.top),
                           radius_bits=bits(source.radius), centre_x=str(source.centre[0]),
                           centre_y=str(source.centre[1]), centre_z=str(source.centre[2]),
                           valid=str(int(source.valid)), system='1', action_end=end))
        rewrite[frame] = dict(collision='geysers=' + source.diagnostic())
        end = str(int(source.step == 30)) if source.phase == 4 else '-1'
        source.tick()
    seed = dict(stage='Test', scenario=1, frame=-2, sources=actors[:1])
    messages = [dict(frame='0', id='source', actor='1', k='r', answer='1', dropped='0',
                     target_bits=bits(100.), speed_bits=bits(100.))]
    evidence = Evidence(geometry, actors, rewrite, seed, messages)
    assert evidence.sources
    contact = dict(actor='1', who='player', host_class='SandGeyser', host='Geyser',
                   self='Foot', self_type='3', other='Raise', other_type='12', kind='r',
                   message='rs::SensorMsgSandGeyserRaise', answer='1')
    rows = {-1: dict(px='0', py='-100', pz='0'), 0: dict(state='SandGeyser/SandGeyser')}
    assert evidence.contact(0, contact, rows, {}, [0, 0, 0])
    for key, value in [('actor', '2'), ('who', 'cap'), ('host', 'Other'), ('host_class', 'Enemy'),
                       ('other', 'Body'), ('other_type', '10'), ('self_type', '2'), ('answer', '0'),
                       ('message', 'rs::SensorMsgPushToPlayer'), ('kind', 'p')]:
        assert not evidence.contact(0, dict(contact, **{key: value}), rows, {}, [0, 0, 0]), key
    assert not Evidence(geometry, actors, rewrite, seed).contact(0, contact, rows, {}, [0, 0, 0])
    assert not evidence.contact(0, contact, {-1: dict(rows[-1], px='216'), 0: rows[0]}, {}, [0, 0, 0])
    host = dict(host='Geyser', sensor='Body', base=(0., -100., 0.))
    assert evidence.floor(0, host, [0, 0, 0])
    assert not evidence.floor(0, dict(host, base=(1., -100., 0.)), [0, 0, 0])
    assert not evidence.floor(0, dict(host, host='Other'), [0, 0, 0])
    assert not evidence.floor(0, dict(host, sensor='Other'), [0, 0, 0])
    assert not evidence.floor(0, host, None)
    for key, value in [('phase', 'UpLoop'), ('step', '999'), ('n', 'unknown'), ('generation', '2'),
                       ('top_local_bits', bits(7.)), ('valid', '0'), ('system', '0'), ('clipped', '1'),
                       ('registry_dropped', '1'), ('scale_bits', ''), ('quat_bits', ''), ('action_end', '1')]:
        invalid = deepcopy(actors)
        invalid[2][key] = value
        assert not Evidence(geometry, invalid, rewrite, seed, messages).sources, key
    assert not Evidence(geometry, actors[1:], rewrite, seed, messages).sources
    assert not Evidence(geometry, actors, {}, seed, messages).sources
    assert not Evidence(geometry, actors, rewrite, dict(seed, stage='Other'), messages).sources
    assert not Evidence(geometry, actors, rewrite, seed, [dict(messages[0], speed_bits=bits(99.))]).sources
    invalid = deepcopy(geometry)
    invalid['sand_geysers'][0]['up_time'] = 0
    assert not Evidence(invalid, actors, rewrite, seed, messages).sources
    carry_rows = {-1: dict(px='0', pz='0'), 0: dict(px='0', pz='0', vx='0', vz='0')}
    assert evidence.carries(0, host, [0, 0, 0], carry_rows, 1.)
    carry_rows[0]['px'] = '2'
    assert not evidence.carries(0, host, [0, 0, 0], carry_rows, 1.)


if __name__ == '__main__':
    self_check()
    print('SandGeyser source/evidence checks passed.')
