"""Add obj473's original placement and skeletal curves to a local geometry export.

Back up the geometry first. No recording, including Mario's trace, is an input.
"""
import argparse
import json
import struct
from pathlib import Path


def curves(path):
    data, at = path.read_bytes(), 8
    if data[:8] != b'PJVCURV1':
        raise ValueError('Expected original skeletal curves')

    def read(fmt):
        nonlocal at
        result = struct.unpack_from('<'+fmt, data, at)
        at += struct.calcsize('<'+fmt)
        return result[0] if len(result) == 1 else result

    def text():
        nonlocal at
        length = read('H')
        value = data[at:at+length].decode('utf8')
        at += length
        return value

    joints = []
    for _ in range(read('I')):
        name = text()
        parent, compensated = read('iB')
        joints.append(dict(name=name, parent=parent, compensated=compensated, bind=read('10f')))
    clips = {}
    for _ in range(read('I')):
        name, length, count = text(), read('f'), read('I')
        records = []
        for _ in range(count):
            joint, rotation = read('I'), read('B')
            channels = []
            for _ in range(10):
                keys = read('I')
                channels.append([read('5f') for _ in range(keys)] if keys else read('f'))
            if joints[joint]['name'] in ('nw4f_root', 'AllRoot', 'JointRoot'):
                records.append(dict(joint=joint, quaternion=rotation, channels=channels))
        if name in ('SwimWait', 'PressDown'):
            clips[name] = dict(length=length, records=records)
    if set(clips) != {'SwimWait', 'PressDown'}:
        raise ValueError('Missing source clips')
    return dict(joints=joints[:3], clips=clips)


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('geometry', type=Path)
    ap.add_argument('--placement', type=Path, required=True)
    ap.add_argument('--curves', type=Path, required=True)
    args = ap.parse_args()
    geometry = json.loads(args.geometry.read_text(encoding='utf8'))
    raw = json.loads(args.placement.read_text(encoding='utf8'))
    if (geometry['stage'], geometry['scenario']) != ('SeaWorldUtsuboCaveStage', 1) or (raw['stage'], raw['scenario']) != (geometry['stage'], geometry['scenario']):
        raise ValueError('Pukupuku stage/scenario mismatch')
    obj = next(o for o in raw['objects'] if o['Id'] == 'obj473')
    rail = obj['Links']['Rail'][0]
    if (obj['UnitConfigName'] != 'Pukupuku' or obj.get('MoveType', 0) != 0 or obj['IsRevive'] is not True
            or rail['Id'] != 'rail15' or not rail['IsClosed'] or len(rail['RailPoints']) != 2
            or [obj['Rotate'][a] for a in 'XYZ'] != [0, -90, 0]
            or [obj['Scale'][a] for a in 'XYZ'] != [1, 1, 1]):
        raise ValueError('Unsupported Pukupuku placement')
    geometry['pukupuku'] = dict(id='obj473', rail_id='rail15', move_type=0, revive=True,
        position=[obj['Translate'][a] for a in 'XYZ'],
        rail=[dict(position=[p['Translate'][a] for a in 'XYZ'],
                   **{'in': [p['ControlPoints'][0][a] for a in 'XYZ']},
                   out=[p['ControlPoints'][1][a] for a in 'XYZ']) for p in rail['RailPoints']],
        animation=curves(args.curves))
    args.geometry.write_text(json.dumps(geometry, separators=(',', ':')), encoding='utf8')


if __name__ == '__main__':
    main()
