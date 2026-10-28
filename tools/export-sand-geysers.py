"""Add upright SandGeyser placements to an existing raw-KCL geometry export.

The independently recorded actor CSV supplies the initial source state only.
Mario's trace is never an input. Back up geometry locally before running.
"""
import argparse
import csv
import importlib.util
import json
import struct
from pathlib import Path


def top_curves(path):
    data = path.read_bytes()
    if data[:8] != b'PJVCURV1':
        raise ValueError('Expected original PJVCURV1 curves, not a baked GLB')
    at = 8

    def read(fmt):
        nonlocal at
        values = struct.unpack_from('<' + fmt, data, at)
        at += struct.calcsize('<' + fmt)
        return values[0] if len(values) == 1 else values

    def text():
        nonlocal at
        length = read('H')
        value = data[at:at + length].decode('utf8')
        at += length
        return value

    joints, parent_y, result = [], None, None
    for _ in range(read('I')):
        name = text()
        joints.append(name)
        read('iB')
        bind = read('10f')
        if name == 'JointRoot':
            parent_y = bind[1]
    for _ in range(read('I')):
        name, length, count = text(), read('f'), read('I')
        for _ in range(count):
            joint, rotation = read('I'), read('B')
            channels = []
            for _ in range(10):
                keys = read('I')
                channels.append([read('5f') for _ in range(keys)] if keys else read('f'))
            if name == 'UpStart' and joints[joint] == 'Top':
                if rotation != 0 or channels[:7] != [1, 1, 1, 0, 0, 0, 1] or length != 15:
                    raise ValueError('Unsupported Top transform')
                result = channels[7:]
    if parent_y is None or result is None:
        raise ValueError('Missing original Top curves')
    return dict(parent_y=parent_y, top_translation=result)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('geometry', type=Path)
    parser.add_argument('--romfs', type=Path, required=True)
    parser.add_argument('--phase', type=Path, required=True)
    parser.add_argument('--curves', type=Path, required=True)
    args = parser.parse_args()
    spec = importlib.util.spec_from_file_location('outfits', Path(__file__).with_name('convert-outfits.py'))
    outfits = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(outfits)
    geometry = json.loads(args.geometry.read_text(encoding='utf8'))
    stage = geometry['stage']
    files = outfits.archive(args.romfs / 'StageData' / (stage + 'Map.szs'))
    placement = outfits.byml(files[stage + 'Map.byml'])[geometry['scenario'] - 1]
    sources = []
    for obj in placement.get('ObjectList', []):
        if obj['UnitConfigName'] != 'SandGeyser':
            continue
        if any(obj['Rotate'][a] != 0 or obj['Scale'][a] != 1 for a in 'XYZ'):
            raise ValueError('Only upright unit-scale geysers are implemented')
        part = next((p for p in geometry['objects'] if p['id'] == obj['Id']), None)
        if part is None:
            raise ValueError('Every source needs its raw Top KCL export')
        target, = obj['Links']['KeyMoveNext']
        sources.append(dict(id=obj['Id'], host=obj['UnitConfig']['DisplayName'],
                            position=[obj['Translate'][a] for a in 'XYZ'],
                            target_y=target['Translate']['Y'], up_start=obj['UpStartTime'],
                            up_time=obj['UpTime'], end_wait=obj.get('EndWaitTime', 300),
                            place_type=obj['PlaceType']))
    rows = list(csv.DictReader(args.phase.open(encoding='utf8')))
    first = min(int(r['frame']) for r in rows)
    seeds = [r for r in rows if int(r['frame']) == first]
    for source in sources:
        seed, = [r for r in seeds if r['id'] == source['id']]
        if ([float(seed['init_' + a]) for a in 'xyz'] != source['position']
                or seed['phase'] != 'Wait' or seed['step'] != '0'
                or seed['top_local_bits'] != '00000000'
                or seed['radius_bits'] != '43e10000'
                or any(seed[k] != '0' for k in ('dead', 'clipped', 'registry_dropped'))
                or any(seed[k] != '1' for k in ('valid', 'system'))):
            raise ValueError('Unsupported or inconsistent independent source seed')
    geometry['sand_geysers'] = sources
    geometry['sand_geyser_curves'] = top_curves(args.curves)
    args.geometry.write_text(json.dumps(geometry, separators=(',', ':')), encoding='utf8')
    seed_path = args.phase.with_name('game.geyser_seed.json')
    seed_path.write_text(json.dumps(dict(stage=stage, scenario=geometry['scenario'], frame=first,
                                         sources=seeds), indent=2), encoding='utf8')
    print(f'{len(sources)} sources; independent seed {seed_path}')


if __name__ == '__main__':
    main()
