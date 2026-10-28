"""Placement-bound RocketFlower equipment evidence. Unknown actors remain refused."""
import math
from sand_geyser import bits, f32


PHASES = ('Wait', 'Follow', 'WaitAttach', 'Attach')
NERVES = ('1d27650', '1d27668', '1d27670', '1d27658')


def vector(row, key):
    return [float(row[key + '_' + a]) for a in 'xyz']


def overlaps(a, b, radius):
    d = [f32(x - y) for x, y in zip(a, b)]
    return f32(f32(f32(d[0]*d[0]) + f32(d[1]*d[1])) + f32(d[2]*d[2])) < f32(radius*radius)


class Evidence:
    def __init__(self, geometry, actors, watcher, seed, rewrite, rows, preroll, start):
        self.frames, self.source, self.token, self.watch_token = {}, None, None, None
        try:
            self.verify(geometry, actors, watcher, seed, rewrite, {**preroll, **rows}, start)
        except (ValueError, KeyError, TypeError, IndexError, StopIteration, OverflowError):
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
        first, last = min(samples), max(samples)
        if first >= 0 or seed['frame'] != first or seed['watcher'] != watches[first]:
            return
        initial = seed['flowers'][0]
        if (initial['id'] != source['id'] or initial['position'] != source['position']
                or initial['sensors'] != list(samples[first].values())):
            return
        phase, step, watch_step, dash, count, stops = 0, int(initial['step']), int(seed['watcher_step']), 0, 0, 0
        frames, token, identity = {}, None, None
        for frame in range(first, last + 1):
            body, equip = samples[frame]['Body'], samples[frame]['Equip']
            w = watches[frame]
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
                if flower[:3] != [source['id'], str(phase), str(step)] or flower[-1] != equip['valid']:
                    return
                force = next(x for x in rewrite[frame]['counters'].split('|') if x.startswith('forcerun='))
                expected_force = '1/38.000000' if dash else ('0/38.000000' if ended or stops and frame > first and phase == 0 else '0/0.000000')
                # The stored speed survives equipment expiry.
                if force != 'forcerun=' + expected_force:
                    return
            frames[frame] = dict(body=body, equip=equip, watcher=w, pickup=pickup, request=request)
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
        return ((c['kind'] == 'a' and not c['message'] and not c['answer']) or
                (c['kind'] == 'r' and c['self'] == 'Carry' and sample['request'] == 1
                 and c['message'] == 'rs::SensorMsgRocketFlowerExtension' and c['answer'] == '0'))
