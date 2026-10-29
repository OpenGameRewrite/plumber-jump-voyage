"""Export and independently simulate the two SandWorldPressExStage crusher placements.

The raw placement sidecar comes from the user's local stage extraction. No Mario
trace is an input to the exporter or the source timeline.
"""
import json
import struct
from pathlib import Path


def f32(x):
    return struct.unpack('<f', struct.pack('<f', x))[0]


def bits(x):
    return struct.pack('>f', x).hex()


def export(geometry, raw, hosts):
    if (geometry['stage'], geometry['scenario']) != ('SandWorldPressExStage', 1) or (raw['stage'], raw['scenario']) != (geometry['stage'], geometry['scenario']):
        raise ValueError('Press stage/scenario mismatch')
    sources = []
    for identity in ('obj165', 'obj167'):
        part = next(p for p in raw['parts'] if p['id'] == identity)
        obj = next(o for o in geometry['objects'] if o['id'] == identity)
        if part['class'] != 'KeyMoveMapParts' or part['model'] != obj['model'] or len(part['key_chain']) != 2:
            raise ValueError('Press placement identity')
        keys = []
        for key in part['key_chain']:
            if (key.get('MoveType', 0) != 0 or key.get('InterpolateType', 0) != 0
                    or any(key.get(k) for k in ('IsFloorTouchStart', 'IsHipDropStart', 'IsStopKill', 'IsPlaySign', 'DelayTime'))
                    or any(key['Rotate'][a] != 0 or key['Scale'][a] != 1 for a in 'XYZ')
                    or any(not isinstance(key[k], int) or key[k] <= 0 for k in ('WaitTime', 'SpeedByTime'))):
                raise ValueError('unsupported key-move placement')
            keys.append(dict(position=[key['Translate'][a] for a in 'XYZ'],
                             WaitTime=key['WaitTime'], SpeedByTime=key['SpeedByTime']))
        if obj['translate'] != keys[0]['position'] or obj['static'] is not False:
            raise ValueError('key-move raw collision pose')
        # A vertical source keeps the collision mesh's X/Z broadphase cells valid.
        if any(keys[0]['position'][i] != keys[1]['position'][i] for i in (0, 2)):
            raise ValueError('only vertical keys are implemented')
        sources.append(dict(id=identity, model=obj['model'], host=hosts[identity], keys=keys, **{'class': part['class']}))
    geometry['key_move_parts'] = sources


class Source:
    def __init__(self, placement):
        self.p = placement
        self.reset()

    def reset(self):
        self.moving, self.key, self.step = False, 0, 0
        self.position = list(self.p['keys'][0]['position'])
        self.move_time = 0

    def seed(self, row):
        self.moving, self.key, self.step = row['phase'] == 'Move', int(row['key_idx']), int(row['step'])
        self.move_time = int(row['move_time'])
        self.position = [struct.unpack('>f', bytes.fromhex(s))[0] for s in row['pos_bits'].split(',')]

    def tick(self):
        key = self.p['keys'][self.key]
        if self.moving:
            self.move_time = key['SpeedByTime']
            t = f32(self.step / key['SpeedByTime'])
            self.position = [f32(a + f32(f32(b - a) * t)) for a, b in zip(key['position'], self.p['keys'][1 - self.key]['position'])]
            if self.step >= key['SpeedByTime']:
                self.key, self.step, self.moving = 1 - self.key, 0, False
                return
        elif self.step >= key['WaitTime']:
            self.moving, self.step = True, 0
            return
        self.step += 1

    def diagnostic(self):
        return f"{self.p['id']}/{'Move' if self.moving else 'Wait'}/{self.key}/{self.step}/" + '/'.join(bits(x) for x in self.position)


class Evidence:
    """Accept only placement-bound sources whose complete independent timeline replays.

    Mario's samples establish the cause and stage-reload boundary for the rule;
    they never supply the source's movement or the rewrite's Press transition.
    """
    def __init__(self, geometry, actors, mario, rewrite, rows, preroll, start):
        self.surfaces, self.episodes = set(), set()
        try:
            self._verify(geometry, actors, mario, rewrite, rows, preroll, start)
        except (KeyError, TypeError, ValueError, IndexError, StopIteration, OverflowError, ZeroDivisionError):
            self.surfaces, self.episodes = set(), set()

    def _verify(self, geometry, actors, mario, rewrite, rows, preroll, start):
        if not geometry or (geometry.get('stage'), geometry.get('scenario')) != ('SandWorldPressExStage', 1) or not actors or not mario or not start:
            return
        from environment_rule import parse_hit_hosts, key_values
        placements = geometry.get('key_move_parts', [])
        if {p['id'] for p in placements} != {'obj165', 'obj167'} or len(placements) != 2:
            return
        sources = {p['id']: Source(p) for p in placements}
        objects = {p['id']: next(o for o in geometry['objects'] if o['id'] == p['id']) for p in placements}
        for p in placements:
            obj = objects[p['id']]
            if (obj['model'] != 'SandWorldPressExKeyMoveParts000' or p['model'] != obj['model'] or p.get('class') != 'KeyMoveMapParts'
                    or obj['static'] is not False or obj['translate'] != p['keys'][0]['position']
                    or obj['rotate'] != [0, 0, 0] or obj['scale'] != [1, 1, 1]
                    or len(p['keys']) != 2 or not geometry['models'][obj['model'] + '#0']['prisms']):
                return
            triangles = [t for t in geometry['triangles'] if geometry['objects'][t[13]]['id'] == p['id']]
            press = [t for t in triangles if geometry['attributes'][t[12]]['FloorCode'] == 'Press']
            if len(triangles) != 12 or len(press) != 4 or any(abs(t[10]) != 1 for t in press):
                return
        by_frame = {}
        for r in actors:
            if r['id'] in sources:
                by_frame.setdefault(int(r['frame']), []).append(r)
        first, last = min(by_frame), max(by_frame)
        if first >= 0 or last != max(rows):
            return
        all_rows = dict(preroll or {}); all_rows.update(rows)
        crush = {int(r['frame']): r for r in mario}
        if len(crush) != len(mario) or set(crush) != set(range(first, last + 1)):
            return
        generations, surfaces, episodes = {}, set(), set()
        cause = False
        for frame in range(first, last + 1):
            samples = by_frame.get(frame, [])
            if {r['id'] for r in samples} != set(sources) or len(samples) != 2:
                return
            row = all_rows.get(frame, {})
            previous = all_rows.get(frame - 1, {})
            reload = (frame > 0 and float(row['game_frame']) > float(previous['game_frame']) + 1
                      and previous['state'] == 'Dead' and row['state'].startswith('Wait/'))
            for r in samples:
                source = sources[r['id']]; p = source.p; obj = objects[r['id']]
                if frame == first:
                    source.seed(r)
                    # The sample observes the incremented keeper step and the
                    # preceding executed step's translation (pending step0 is a key).
                    expected = list(p['keys'][source.key]['position'])
                    if source.moving and source.step > 0:
                        t = f32((source.step - 1) / p['keys'][source.key]['SpeedByTime'])
                        expected = [f32(a + f32(f32(b-a) * t)) for a,b in zip(expected, p['keys'][1-source.key]['position'])]
                    if source.position != expected:
                        return
                elif reload:
                    source.reset(); source.tick()
                    if int(r['generation']) <= int(generations[r['id']]):
                        return
                elif r['generation'] != generations[r['id']]:
                    return
                generations[r['id']] = r['generation']
                if (any(r[k] != '0' for k in ('dead', 'clipped', 'registry_dropped', 'key_stop', 'key_restart', 'delay_time'))
                        or r['key_count'] != '2' or r['going_to_end'] != '1' or r['zone'] != ''
                        or r['phase'] != ('Move' if source.moving else 'Wait') or int(r['step']) != source.step
                        or int(r['key_idx']) != source.key
                        or r['n_vt'] != ('1defac8' if source.moving else '1defa48')
                        or int(r['wait_time']) != p['keys'][source.key]['WaitTime']
                        or int(r['move_time']) != source.move_time
                        or r['init_bits'] != ','.join(bits(x) for x in obj['translate'])
                        or r['pos_bits'] != ','.join(bits(x) for x in source.position)
                        or r['quat_bits'] != '00000000,00000000,00000000,3f800000'):
                    return
                if frame >= 0:
                    diag = key_values(rewrite.get(frame, {}).get('collision', '')).get('keyparts', '').split('|')
                    if source.diagnostic() not in diag:
                        return
                for surface, host in parse_hit_hosts(row.get('hit_hosts')).items():
                    if (host['host'] == obj.get('display_name', '砂のワールドプレスExキームーブパーツ000')
                            and host['sensor'] == 'Collision' and host['sensor_type'] == '15'
                            and host['base'] == tuple(round(f32(x-s), 6) for x,s in zip(source.position, start))):
                        surfaces.add((frame, surface, host['host'], host['base']))
            m = crush[frame]
            if any(m[k] not in ('0', '1') for k in ('flag_a0', 'flag_a1', 'pressed', 'pressed_ground', 'pressed_wall')):
                return
            if (m['flag_a0'], m['flag_a1']) != (m['pressed_ground'], m['pressed_wall']) or int(m['pressed']) != (int(m['flag_a0']) | int(m['flag_a1'])):
                return
            if frame >= 0 and key_values(rewrite.get(frame, {}).get('collision', '')).get('pressed') != m['flag_a0'] + '/' + m['flag_a1']:
                return
            if int(m['gate_true']) < 0 or int(m['gate_queries']) < int(m['gate_true']):
                return
            if reload:
                cause = False
            elif row.get('state', '').startswith('Press/') and not previous.get('state', '').startswith('Press/'):
                old = crush.get(frame - 1, {})
                cause = (old.get('flag_a0') == '1' and int(old.get('gate_true', 0)) > 0
                         and any(s[0] == frame - 1 and s[1] == 'ceiling' for s in surfaces))
            elif not row.get('state', '').startswith(('Press/', 'Dead')):
                cause = False
            if cause:
                episodes.add(frame)
            for source in sources.values():
                source.tick()
        self.surfaces, self.episodes = surfaces, episodes

    def surface(self, frame, surface, host):
        return (frame, surface, host['host'], host['base']) in self.surfaces


def self_check():
    p = dict(id='test', keys=[dict(position=[0., y, 0.], WaitTime=2, SpeedByTime=3) for y in (10., 0.)])
    source = Source(p)
    for _ in range(2 * (2 + 1 + 3 + 1)):
        source.tick()
    assert (source.moving, source.key, source.step, source.position) == (False, 0, 0, [0., 10., 0.])


if __name__ == '__main__':
    import argparse
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('geometry', type=Path)
    parser.add_argument('placements', type=Path)
    parser.add_argument('--romfs', type=Path, required=True)
    args = parser.parse_args()
    self_check()
    geometry = json.loads(args.geometry.read_text(encoding='utf8'))
    import importlib.util
    spec = importlib.util.spec_from_file_location('outfits', Path(__file__).with_name('convert-outfits.py'))
    outfits = importlib.util.module_from_spec(spec); spec.loader.exec_module(outfits)
    files = outfits.archive(args.romfs / 'StageData' / (geometry['stage'] + 'Map.szs'))
    stage = outfits.byml(files[geometry['stage'] + 'Map.byml'])[geometry['scenario'] - 1]
    hosts = {o['Id']: o['UnitConfig']['DisplayName'] for o in stage['ObjectList']}
    export(geometry, json.loads(args.placements.read_text(encoding='utf8')), hosts)
    args.geometry.write_text(json.dumps(geometry, separators=(',', ':')), encoding='utf8')
    print('Exported two placement-bound key-move sources')
