"""Placement-bound RocketFlower equipment evidence. Unknown actors remain refused."""
import math
import struct
from sand_geyser import bits, f32


PHASES = ('Wait', 'Follow', 'WaitAttach', 'Attach')
NERVES = ('1d27650', '1d27668', '1d27670', '1d27658')


def vector(row, key):
    return [float(row[key + '_' + a]) for a in 'xyz']


def overlaps(a, b, radius):
    d = [f32(x - y) for x, y in zip(a, b)]
    return f32(f32(f32(d[0]*d[0]) + f32(d[1]*d[1])) + f32(d[2]*d[2])) < f32(radius*radius)


def checked_centre(row):
    centre = vector(row, 'centre')
    if (not all(math.isfinite(v) for v in centre)
            or row['centre_bits'] != ','.join(bits(v) for v in centre)
            or row['radius_bits'] != bits(70.) or float(row['radius']) != 70.):
        raise ValueError('inconsistent sensor values')
    return centre


class Evidence:
    def __init__(self, geometry, actors, watcher, seed, rewrite, rows, preroll, start):
        self.frames, self.source, self.token, self.watch_token = {}, None, None, None
        try:
            self.verify(geometry, actors, watcher, seed, rewrite, {**preroll, **rows}, start)
        except (ValueError, KeyError, TypeError, IndexError, StopIteration, OverflowError, struct.error):
            self.frames, self.source, self.token, self.watch_token = {}, None, None, None

    def verify(self, geometry, actors, watcher, seed, rewrite, rows, start):
        if not all((geometry, actors, watcher, seed, rewrite, rows, start)):
            return
        sources = geometry.get('rocket_flowers', [])
        if len(sources) != 1 or (seed['stage'], seed['scenario']) != (geometry['stage'], geometry['scenario']):
            return
        source = sources[0]
        placement = source['placement']
        if (placement['Id'] != source['id'] or placement['UnitConfigName'] != 'RocketFlower'
                or source['position'] != [placement['Translate'][a] for a in 'XYZ']
                or source['host'] != placement['UnitConfig']['DisplayName']
                or placement['Scale'] != dict(X=1., Y=1., Z=1.)
                or placement['Rotate']['X'] != 0 or placement['Rotate']['Z'] != 0
                or placement['IsLinkDest']
                or source['sensors'] != [dict(Name='Body', Radius=70., Type='MapObj', Y=50.),
                                        dict(Name='Equip', Radius=70., Type='Npc')]):
            return
        samples = {}
        for r in actors:
            if r['id'] == source['id']:
                frame = int(r['frame'])
                sample = samples.setdefault(frame, {})
                if r['sensor'] in sample:
                    return
                sample[r['sensor']] = r
        watches = {int(r['frame']): r for r in watcher}
        if len(watches) != len(watcher) or len(seed['flowers']) != 1:
            return
        first, last = min(samples), max(samples)
        if first >= 0 or seed['frame'] != first or seed['watcher'] != watches[first]:
            return
        initial = seed['flowers'][0]
        if (initial['id'] != source['id'] or initial['host'] != source['host']
                or initial['position'] != source['position']
                or initial['sensors'] != list(samples[first].values())):
            return
        phase, step, watch_step, dash, count, stops = 0, int(initial['step']), int(seed['watcher_step']), 0, 0, 0
        frames, token, identity = {}, None, None
        for frame in range(first, last + 1):
            body, equip = samples[frame]['Body'], samples[frame]['Equip']
            w = watches[frame]
            for sensor in (body, equip, w):
                checked_centre(sensor)
                if sensor['n'] != (sensor['next_n'] if sensor['next_n'] != '0' else sensor['current_n']):
                    return
            if frame > first:
                step = 1 if step == -1 else step + 1
                watch_step = 1 if watch_step == -1 else watch_step + 1
            pickup, request, ended = int(w['cap_get_accepted']), int(w['equip_accepted']), int(w['end_calls'])
            if pickup:
                if phase != 0 or pickup != 1 or dash:
                    return
                previous = rows[frame - 1]
                cap = [f32(float(previous['cap_p' + a]) + o) for a, o in zip('xyz', start)]
                if previous['cap_state'] not in ('Throw', 'ThrowBrake', 'ThrowStay', 'Return') or not overlaps(cap, vector(body, 'centre'), 110.):
                    return
                phase, step = 1, -1
            elif phase == 1 and step == 1:
                stops += 2
            elif phase == 1 and step == 2:
                watch_step += 2
            if phase == 1 and not pickup and rows[frame]['cap_state'] == 'Catch':
                phase, step = 2, 0
            if request:
                if phase != 2 or request != 1 or w['equip_id'] != source['id']:
                    return
                previous = rows[frame - 1]
                feet = [f32(float(previous['p' + a]) + o) for a, o in zip('xyz', start)]
                if not any(overlaps([feet[0], f32(feet[1] + y), feet[2]],
                                    vector(equip, 'centre'), radius + 70.)
                           for y, radius in ((75., 50.), (110., 40.))):
                    return
                phase, step, dash, count, watch_step = 3, -1, 1, 1, -1
            if dash and not request and watch_step == 240:
                if ended != 1:
                    return
                phase, step, dash, count, watch_step = 0, -1, 0, 0, 0
            elif ended:
                return
            if not dash and frame > first and samples[frame - 1]['Body']['clipped'] == '1':
                step = -1
            if (body['phase'] != PHASES[phase] or equip['phase'] != PHASES[phase]
                    or body['n'] != NERVES[phase] or equip['n'] != NERVES[phase]
                    or int(body['step']) != step or int(equip['step']) != step
                    or int(w['step']) != watch_step or w['phase'] != ('Dash' if dash else 'Wait')
                    or w['n'] != ('1d27b10' if dash else '1d27b08')
                    or w['count'] != str(count) or w['capacity'] != '3'
                    or w['equipment_present'] != str(dash)
                    or w['flowers'] != (source['id'] if count else '')
                    or body['pos_bits'] != ','.join(bits(v) for v in source['position'])
                    or body['centre_bits'] != ','.join(bits(v) for v in [source['position'][0], source['position'][1]+50., source['position'][2]])
                    or any(r['registry_dropped'] != '0' or r['dead'] != '0' or r['present'] != '1'
                           or r['radius_bits'] != bits(70.) for r in (body, equip))
                    or equip['valid'] != str(int(phase in (2, 3))) or body['valid'] != '1'):
                return
            if (body['clipped'] not in ('0', '1') or equip['clipped'] != body['clipped']
                    or any(r['system'] != str(1-int(body['clipped'])) for r in (body, equip))):
                return
            if frame == first:
                identity = body['generation']
                token = body['actor']
            if token == '0' or any(r['actor'] != token or r['generation'] != identity for r in (body, equip)):
                return
            if frame >= 0:
                diagnostic = rewrite[frame].get('collision', '').split('rocket=', 1)[-1].split(';flowers=', 1)
                if len(diagnostic) != 2 or diagnostic[0] != f'{dash}/{watch_step}/{count}/{pickup}/{request}/{ended}/{stops}':
                    return
                flower = diagnostic[1].split('/')
                if (len(flower) != 8 or flower[:3] != [source['id'], str(phase), str(step)]
                        or flower[6:] != [equip['valid'], body['clipped']]):
                    return
                replay_centre = [struct.unpack('!f', bytes.fromhex(v))[0] for v in flower[3:6]]
                if not all(math.isfinite(v) for v in replay_centre):
                    return
                force = next(x for x in rewrite[frame]['counters'].split('|') if x.startswith('forcerun='))
                expected_force = '1/38.000000' if dash else ('0/38.000000' if ended or stops and frame > first and phase == 0 else '0/0.000000')
                # The stored speed survives equipment expiry.
                if force != 'forcerun=' + expected_force:
                    return
            frames[frame] = dict(body=body, equip=equip, watcher=w, pickup=pickup, request=request,
                                 replay_centre=replay_centre if frame >= 0 else None)
        from environment_rule import parse_contacts
        watch_tokens = {c['actor'] for r in rows.values() for c in parse_contacts(r.get('contacts'))
                        if c['host_class'] == 'RocketFlowerEquipWatcher'}
        if len(watch_tokens) > 1 or '0' in watch_tokens:
            return
        self.frames, self.source, self.token = frames, source, token
        self.watch_token = next(iter(watch_tokens), None)

    def contact(self, frame, c, rows, preroll, start):
        if not self.source or not start or frame not in self.frames:
            return False
        sample = self.frames[frame]
        if c['host_class'] == 'RocketFlower' and c['actor'] == self.token and c['host'] == self.source['host']:
            name, kind = c['other'], c['other_type']
            if (name, kind) not in (('Body', '12'), ('Equip', '6')):
                return False
            sensor = sample[name.lower()]
        elif (c['host_class'] == 'RocketFlowerEquipWatcher' and c['actor'] == self.watch_token
              and c['host'] == self.source['host'] + '装備監視' and c['other'] == 'Equip' and c['other_type'] == '6'):
            sensor = sample['watcher']
        else:
            return False
        if sensor['valid'] != '1' or sensor['system'] != '1':
            return False
        if c['who'] == 'cap':
            return (c['host_class'] == 'RocketFlower' and c['other'] == 'Body' and c['kind'] == 'a'
                    and (c['self'], c['self_type']) in (('Attack', '4'), ('Eye', '0'))
                    and not c['message'] and not c['answer'] and sample['equip']['phase'] == 'Follow')
        own = {('Foot','3'):(40.,40.), ('Body','1'):(75.,50.), ('Head','1'):(110.,40.),
               ('Carry','2'):(100.,130.), ('Eye','5'):(80.,800.), ('HipDropKnockDown','2'):(0.,150.)}.get((c['self'], c['self_type']))
        previous = rows.get(frame-1, preroll.get(frame-1))
        if c['who'] != 'player' or not own or not previous:
            return False
        point = [f32(float(previous['p'+a])+o) for a,o in zip('xyz',start)]
        point[1] = f32(point[1]+own[0])
        if not overlaps(point, vector(sensor,'centre'), own[1]+70.):
            return False
        # Prove physical flower contacts against the independently moved source
        # too; a recorded centre alone cannot authorize an unreproduced cause.
        if c['host_class'] == 'RocketFlower':
            centre = ([self.source['position'][0], self.source['position'][1] + 50., self.source['position'][2]]
                      if c['other'] == 'Body' else sample['replay_centre'])
            if not centre or not overlaps(point, centre, own[1]+70.):
                return False
        return ((c['kind'] == 'a' and not c['message'] and not c['answer']) or
                (c['kind'] == 'r' and c['self'] == 'Carry' and sample['request'] == 1
                 and c['message'] == 'rs::SensorMsgRocketFlowerExtension' and c['answer'] == '0'))


def self_check():
    from copy import deepcopy

    assert not Evidence(None, None, None, None, None, {}, {}, None).source

    source = dict(id='flower', host='Flower', position=[0., 0., 0.],
                  sensors=[dict(Name='Body', Radius=70., Type='MapObj', Y=50.),
                           dict(Name='Equip', Radius=70., Type='Npc')],
                  placement=dict(Id='flower', UnitConfigName='RocketFlower',
                                 UnitConfig=dict(DisplayName='Flower'), IsLinkDest=False,
                                 Translate=dict(X=0., Y=0., Z=0.), Rotate=dict(X=0., Y=0., Z=0.),
                                 Scale=dict(X=1., Y=1., Z=1.)))
    geometry = dict(stage='Fixture', scenario=1, rocket_flowers=[source])
    actors, watcher, rows, rewrite = [], [], {}, {}
    for frame in range(-3, 245):
        phase = 0 if frame < -1 or frame >= 242 else 1 if frame < 1 else 2 if frame == 1 else 3
        step = (frame + 4 if frame < -1 else -1 if frame in (-1, 2) or frame >= 242
                else 1 if frame == 0 else 0 if frame == 1 else frame - 2)
        clock = frame + 4 if frame < 1 else 7 if frame == 1 else -1 if frame == 2 else frame - 2 if frame < 242 else frame - 242
        dash, pickup, request, ended = int(2 <= frame < 242), int(frame == -1), int(frame == 2), int(frame == 242)
        valid, clipped, stops = int(phase in (2, 3)), int(frame >= 242), 2 if frame >= 0 else 0
        for sensor, centre in [('Body', [0., 50., 0.]), ('Equip', [0., 100., 0.])]:
            actors.append(dict(frame=str(frame), id='flower', actor='1', generation='1',
                               phase=PHASES[phase], n=NERVES[phase], current_n=NERVES[phase], next_n='0',
                               step=str(step), sensor=sensor, present='1', system=str(1-clipped),
                               valid=str(valid if sensor == 'Equip' else 1), radius='70', radius_bits=bits(70.),
                               pos_bits=','.join(bits(v) for v in source['position']),
                               centre_bits=','.join(bits(v) for v in centre),
                               **dict(zip(('centre_x', 'centre_y', 'centre_z'), map(str, centre))),
                               dead='0', clipped=str(clipped), registry_dropped='0'))
        nerve = '1d27b10' if dash else '1d27b08'
        watcher.append(dict(frame=str(frame), phase='Dash' if dash else 'Wait', n=nerve,
                            current_n=nerve, next_n='0', step=str(clock),
                            count=str(dash), capacity='3', equipment_present=str(dash),
                            flowers='flower' if dash else '', equip_id='flower' if request else 'unknown',
                            cap_get_accepted=str(pickup), equip_accepted=str(request), end_calls=str(ended),
                            valid=str(int(frame > 2)), system='1', radius='70', radius_bits=bits(70.),
                            centre_x='0', centre_y='100', centre_z='0',
                            centre_bits=','.join(bits(v) for v in [0., 100., 0.])))
        rows[frame] = dict(px='0', py='0', pz='0', cap_px='0', cap_py='0', cap_pz='0',
                           cap_state='Throw' if frame < 1 else 'Catch')
        force = '1/38.000000' if dash else '0/38.000000' if frame >= 242 else '0/0.000000'
        rewrite[frame] = dict(counters='forcerun=' + force,
                              collision=f'rocket={dash}/{clock}/{dash}/{pickup}/{request}/{ended}/{stops};'
                              f'flowers=flower/{phase}/{step}/00000000/42c80000/00000000/{valid}/{clipped}')
    seed = dict(stage='Fixture', scenario=1, frame=-3, watcher_step=1, watcher=watcher[0],
                flowers=[dict(id='flower', host='Flower', position=source['position'], step=1, sensors=actors[:2])])

    def evidence(g=geometry, a=actors, w=watcher, s=seed, replay=rewrite):
        return Evidence(g, a, w, s, replay, rows, {}, [0., 0., 0.])

    contact = dict(actor='1', host_class='RocketFlower', host='Flower', other='Equip', other_type='6',
                   who='player', self='Carry', self_type='2', kind='r',
                   message='rs::SensorMsgRocketFlowerExtension', answer='0')
    assert evidence().source and evidence().contact(2, contact, rows, {}, [0., 0., 0.])
    assert not evidence().contact(3, contact, rows, {}, [0., 0., 0.]), 'request frame only'
    for key, value in [('actor', '2'), ('host_class', 'Unknown'), ('host', 'Other'),
                       ('other_type', '12'), ('answer', '1'), ('message', 'rs::SensorMsgPushToPlayer')]:
        assert not evidence().contact(2, {**contact, key: value}, rows, {}, [0., 0., 0.])
    for key, value in [('scenario', 2), ('stage', 'Other')]:
        assert not evidence(s={**seed, key: value}).source
    for change in ('links', 'position', 'sensors'):
        bad = deepcopy(geometry)
        p = bad['rocket_flowers'][0]
        if change == 'links':
            p['placement']['IsLinkDest'] = True
        elif change == 'position':
            p['position'][0] = 1.
        else:
            p['sensors'][0]['Radius'] = 71.
        assert not evidence(g=bad).source
    assert not evidence(replay={}).source
    assert not evidence(w=watcher + [watcher[-1]]).source
    assert not evidence(a=actors + [actors[-1]]).source
    for key, value in [('step', '9'), ('actor', '2'), ('centre_bits', 'bad'),
                       ('generation', '2'), ('radius_bits', bits(71.)), ('next_n', 'bad')]:
        bad = deepcopy(actors)
        bad[-1][key] = value
        assert not evidence(a=bad).source, key
    bad = deepcopy(watcher)
    bad[-3]['end_calls'] = '0'
    assert not evidence(w=bad).source, 'expiry must be independently observed'
    bad = deepcopy(rewrite)
    bad[2]['collision'] = bad[2]['collision'].replace('/00000000/42c80000/', '/bad/42c80000/')
    assert not evidence(replay=bad).source
