"""Evidence for fixed, unconnected ceiling bars; no moving-bar exemption."""
import math
from sand_geyser import f32, bits


def dot(a, b):
    return f32(f32(f32(a[0] * b[0]) + f32(a[1] * b[1])) + f32(a[2] * b[2]))


def force(centre, body):
    delta = [f32(a - b) for a, b in zip(centre, body)]
    length = f32(math.sqrt(dot(delta, delta)))
    inverse = f32(1. / length) if length >= .001 else 0.
    power = max(f32(length * .25), min(length, 40.))
    return [f32(f32(v * inverse) * power) for v in delta]


def feet(row, start):
    return [f32(float(row[k]) + o) for k, o in zip(('px', 'py', 'pz'), start)]


class Evidence:
    def __init__(self, geometry, actors, rewrite, seed, messages, rows, preroll, start):
        self.sources, self.tokens, self.frames, self.messages = {}, {}, {}, set()
        try:
            self.verify(geometry, actors, rewrite, seed, messages, {**preroll, **rows}, start)
        except (KeyError, ValueError, TypeError, IndexError, OverflowError, ZeroDivisionError, StopIteration):
            self.sources, self.tokens, self.frames, self.messages = {}, {}, {}, set()

    def verify(self, geometry, actors, rewrite, seed, messages, rows, start):
        if not geometry or not actors or not seed or not start:
            return
        sources = {p['id']: p for p in geometry.get('grab_ceil_bars', [])}
        if not sources or (seed['stage'], seed['scenario']) != (geometry['stage'], geometry['scenario']):
            return
        for p in sources.values():
            part = next(o for o in geometry['objects'] if o['id'] == p['id'])
            if (part.get('static') is not True or part['model'] != 'PoleGrabCeil'
                    or part['translate'] != p['position'] or part['rotate'] != [0, 0, 0]
                    or part['scale'] != [1, 1, 1] or p['side'] != [1, 0, 0]
                    or p['radius'] != 150 or p['sensor'] != 'Snap' or p['type'] != 12
                    or not geometry.get('models', {}).get('PoleGrabCeil#0', {}).get('prisms')):
                return
        by_frame = {}
        for r in actors:
            if r['id'] in sources:
                by_frame.setdefault(int(r['frame']), {})[r['id']] = r
        first, last = min(by_frame), max(by_frame)
        if first >= 0 or seed['frame'] != first or seed['sources'] != list(by_frame[first].values()):
            return
        phases, steps, centres, systems, generations, tokens, frames = {}, {}, {}, {}, {}, {}, {}
        for identity, p in sources.items():
            r = by_frame[first][identity]
            if r['phase'] != 'Wait' or r['n'] != '1d1f390':
                return
            phases[identity], steps[identity] = 0, int(r['step'])
            centres[identity] = [float(r['centre_' + a]) for a in 'xyz']
            systems[identity], generations[identity] = r['system'], r['generation']
        attached = None
        for frame in range(first, last + 1):
            sample = by_frame[frame]
            if set(sample) != set(sources):
                return
            row = rows[frame]
            # Mario's state sends start/end; identify its part by the exported base,
            # not by a scenario's name, frame numbers or a shared actor display name.
            if row.get('state', '').startswith('GrabCeil/') and attached is None:
                from environment_rule import parse_hit_hosts
                for host in parse_hit_hosts(row.get('hit_hosts')).values():
                    attached = next((i for i, p in sources.items() if host['base'] == tuple(
                        round(f32(v - o), 6) for v, o in zip(p['position'], start))), None)
                    if attached:
                        break
            elif not row.get('state', '').startswith('GrabCeil/'):
                attached = None
            diagnostic = []
            for identity, p in sources.items():
                r = sample[identity]
                phase, step = phases[identity], steps[identity]
                if systems[identity] == '1':
                    if phase == 0 and identity == attached:
                        phase, step = 1, -1
                    elif phase == 1 and identity != attached:
                        phase, step = 2, -1
                centre = centres[identity]
                if (r['phase'] != ['Wait', 'Grab', 'GrabEnd'][phase]
                        or r['n'] != ['1d1f390', '1d1f398', '1d1f3a0'][phase] or int(r['step']) != step
                        or r['generation'] != generations[identity] or r['system'] != systems[identity]
                        or r['pos_bits'] != ','.join(bits(v) for v in p['position'])
                        or r['radius_bits'] != bits(150.) or r['centre_bits'] != ','.join(bits(v) for v in centre)
                        or any(r[k] != '0' for k in ('dead', 'registry_dropped'))
                        or r['sensor'] != '1' or r['valid'] != '1'):
                    return
                if r['actor'] != '0':
                    if r['actor'] in tokens and tokens[r['actor']] != identity:
                        return
                    tokens[r['actor']] = identity
                frames[frame, identity] = (phase, centre, systems[identity] == '1')
                diagnostic.append(f"{identity}/{phase}/{step}/{'/'.join(bits(v) for v in centre)}/{systems[identity]}")
                if systems[identity] == '1':
                    at = feet(row, start)
                    along = min(max(f32(at[0] - p['position'][0]), 0.), 380.)
                    centres[identity] = [f32(p['position'][0] + along), *p['position'][1:]]
                    if phase == 2 and step > 30:
                        phases[identity], steps[identity] = 0, 0
                    else:
                        phases[identity], steps[identity] = phase, 1 if step == -1 else step + 1
            if frame >= 0 and 'grabceil=' + '|'.join(diagnostic) not in rewrite[frame].get('collision', ''):
                return
        proved = set()
        for r in messages or []:
            identity, frame = tokens.get(r['actor']), int(r['frame'])
            if identity != r['id'] or r['dropped'] != '0' or r['k'] != 'r':
                return
            state = frames[frame - 1, identity]
            body = feet(rows[frame - 1], start)
            body[1] = f32(body[1] + 75.)
            if (state[0] != 0 or not state[2]
                    or r['force_bits'] != ','.join(bits(v) for v in force(state[1], body))):
                return
            proved.add((frame, r['actor'], r['k'], r['answer']))
        self.sources, self.tokens, self.frames, self.messages = sources, tokens, frames, proved

    def contact(self, frame, c, rows, preroll, start):
        identity = self.tokens.get(c['actor'])
        p = self.sources.get(identity)
        if (not p or not start or c['who'] != 'player' or c['host_class'] != 'PoleGrabCeil'
                or c['host'] != p['host'] or c['other'] != 'Snap' or c['other_type'] != '12'):
            return False
        own = {('Foot', '3'): (40., 0., 40.), ('Body', '1'): (75., 0., 50.),
               ('Head', '1'): (110., 0., 40.), ('Carry', '2'): (100., 0., 130.),
               ('HipDropKnockDown', '2'): (0., 0., 150.), ('Eye', '5'): (80., 0., 800.),
               ('WallGrabHead', '1'): (-10., -40., 40.), ('WallGrabBody', '1'): (-70., -20., 50.),
               ('WallGrabFoot', '3'): (-110., -15., 30.)}.get((c['self'], c['self_type']))
        previous = rows.get(frame - 1, preroll.get(frame - 1, {}))
        state = self.frames.get((frame - 1, identity))
        if not own or not previous or not state or not state[2]:
            return False
        at = feet(previous, start)
        # HitSensor's local offset follows the full actor matrix (Eye is world-up).
        if c['self'] == 'Eye':
            offset = [0., own[0], 0.]
        else:
            x, y, z, w = (float(previous[k]) for k in ('qx', 'qy', 'qz', 'qw'))
            up = [2*(x*y-z*w), 1-2*(x*x+z*z), 2*(y*z+x*w)]
            front = [2*(x*z+y*w), 2*(y*z-x*w), 1-2*(x*x+y*y)]
            offset = [f32(f32(u*own[0]) + f32(v*own[1])) for u,v in zip(up, front)]
        at = [f32(a+b) for a,b in zip(at,offset)]
        delta = [f32(a - b) for a, b in zip(at, state[1])]
        if dot(delta, delta) >= f32((own[2] + 150.) ** 2):
            return False
        if c['kind'] == 'a':
            return not c['message'] and not c['answer']
        return (c['message'] == 'rs::SensorMsgObjSnapForce' and c['kind'] == 'r'
                and (frame, c['actor'], c['kind'], c['answer']) in self.messages
                and c['answer'] == str(int(c['self_type'] in ('1', '3'))))


def self_check():
    from copy import deepcopy

    assert force([0., 200., 0.], [0., 0., 0.]) == [0., 50., 0.]
    assert force([0., 100., 0.], [0., 0., 0.]) == [0., 40., 0.]
    empty = Evidence(None, None, {}, None, None, {}, {}, None)
    assert not empty.contact(0, {'actor': '1'}, {}, {}, None)
    p = dict(id='bar', host='Bar', position=[0., 0., 0.], side=[1., 0., 0.],
             radius=150., sensor='Snap', type=12)
    g = dict(stage='Fixture', scenario=1, grab_ceil_bars=[p],
             objects=[dict(id='bar', model='PoleGrabCeil', translate=p['position'],
                           rotate=[0, 0, 0], scale=[1, 1, 1], static=True)],
             models={'PoleGrabCeil#0': dict(prisms=[[1]])})
    centre = [200., 0., 0.]
    actors, rows, rewrite = [], {}, {}
    for frame in range(-2, 3):
        actors.append(dict(frame=str(frame), id='bar', actor='1', phase='Wait', n='1d1f390',
                           step=str(frame+3), generation='1', system='1', dead='0', registry_dropped='0',
                           sensor='1', valid='1', pos_bits=','.join(bits(v) for v in p['position']),
                           radius_bits=bits(150.), centre_bits=','.join(bits(v) for v in centre),
                           **dict(zip(('centre_x', 'centre_y', 'centre_z'), map(str, centre)))))
        rows[frame] = dict(px='200', py='-200', pz='0', state='Jump/Jump', fx='0', fz='1',
                           qx='0', qy='0', qz='0', qw='1')
        rewrite[frame] = dict(collision=f"grabceil=bar/0/{frame+3}/{'/'.join(bits(v) for v in centre)}/1")
    seed = dict(stage='Fixture', scenario=1, frame=-2, sources=[actors[0]])
    messages = [dict(id='bar', actor='1', frame='0', k='r', answer='1', dropped='0',
                     force_bits=','.join(bits(v) for v in force(centre, [200., -125., 0.])))]
    def evidence(geometry=g, actor_rows=actors, replay=rewrite, initial=seed, received=messages):
        return Evidence(geometry, actor_rows, replay, initial, received, rows, {}, [0., 0., 0.])
    c = dict(actor='1', who='player', host_class='PoleGrabCeil', host='Bar', other='Snap',
             other_type='12', self='Head', self_type='1', kind='r', answer='1',
             message='rs::SensorMsgObjSnapForce')
    assert evidence().contact(0, c, rows, {}, [0., 0., 0.])
    for key, value in [('actor', '2'), ('host_class', 'PoleGrabCeilKeyMoveParts'),
                       ('other_type', '13'), ('who', 'cap'), ('answer', '0')]:
        assert not evidence().contact(0, {**c, key: value}, rows, {}, [0., 0., 0.])
    for altered in [None, {**seed, 'scenario': 2}]:
        assert not evidence(initial=altered).sources
    bad = deepcopy(g)
    bad['objects'][0]['model'] = 'PoleGrabCeilKeyMoveParts'
    assert not evidence(geometry=bad).sources
    assert not evidence(replay={}).sources
    assert not evidence(received=[{**messages[0], 'force_bits': '00000000,00000000,00000000'}]).sources
    bad = deepcopy(actors)
    bad[-1]['step'] = '400'
    assert not evidence(actor_rows=bad).sources
