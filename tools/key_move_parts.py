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


def export(geometry, raw):
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
        sources.append(dict(id=identity, model=obj['model'], keys=keys))
    geometry['key_move_parts'] = sources


class Source:
    def __init__(self, placement):
        self.p = placement
        self.reset()

    def reset(self):
        self.moving, self.key, self.step = False, 0, 0
        self.position = list(self.p['keys'][0]['position'])

    def seed(self, row):
        self.moving, self.key, self.step = row['phase'] == 'Move', int(row['key_idx']), int(row['step'])
        self.position = [struct.unpack('>f', bytes.fromhex(s))[0] for s in row['pos_bits'].split(',')]

    def tick(self):
        key = self.p['keys'][self.key]
        if self.moving:
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
    args = parser.parse_args()
    self_check()
    geometry = json.loads(args.geometry.read_text(encoding='utf8'))
    export(geometry, json.loads(args.placements.read_text(encoding='utf8')))
    args.geometry.write_text(json.dumps(geometry, separators=(',', ':')), encoding='utf8')
    print('Exported two placement-bound key-move sources')
