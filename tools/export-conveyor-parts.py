"""Add a ConveyerMapParts placement (a chain of sliding collision steps) to an existing raw-KCL geometry export.

al::ConveyerMapParts::init (al/Library/MapObj/ConveyerMapParts.cpp) creates one ConveyerStep actor per
`int(totalMoveDistance / PartsInterval) + 1`; each step is the placement's own collision part. The geometry
export holds the placement once, as one object; this tool replaces it by one object per step (ids
`<id>/<index>`), placed where the host's init puts them (`setTransAndResetByCoord(i * interval + offset)`),
and records the arguments the rewrite reproduces. Only the arguments the rewrite supports are accepted: an
unrotated unit-scale placement that slides along its local Z axis (MoveAxis 2) through unrotated
KeyMoveNext keys with linear interpolation, no switch, no ride-only mode, no key actions.
The recorded actor CSV is never an input here: the replay seeds the offset from it at run time and
tools/conveyor_parts.py re-simulates the whole timeline from the placement to check it.
Back up the geometry locally before running.

    py tools/export-conveyor-parts.py geometry/SandWorldPyramid001Stage.s1.json --romfs <romfs> --id obj243
"""
import argparse
import copy
import importlib.util
import json
import struct
from pathlib import Path

# ConveyerMapParts.cpp registerConveyerSteps: every step is a `ConveyerStep("コンベア足場")`, the name the
# probe's typed contact log reports for the part under the player.
STEP_HOST = 'コンベア足場'
SWITCH_KEYS = ('SwitchStart', 'SwitchAppear', 'SwitchA', 'SwitchB', 'SwitchStop', 'SwitchKeep')


def f32(x):
    return struct.unpack('<f', struct.pack('<f', x))[0]


def vec(t):
    return [f32(t[a]) for a in 'XYZ']


def conveyor(obj):
    config = obj['UnitConfig']
    if config.get('ParameterConfigName') != 'ConveyerMapParts':
        raise ValueError(f"{obj['Id']} is not a ConveyerMapParts")
    if any(obj['Rotate'][a] != 0 or obj['Scale'][a] != 1 for a in 'XYZ'):
        raise ValueError(f"{obj['Id']}: only unrotated unit-scale placements are implemented")
    if obj.get('MoveAxis', 2) != 2:
        raise ValueError(f"{obj['Id']}: only MoveAxis 2 (local Z) is implemented")
    if obj.get('IsRideOnlyMove') or any(key in obj for key in SWITCH_KEYS):
        raise ValueError(f"{obj['Id']}: switch and ride-only conveyors are not implemented")
    links = obj.get('Links', {})
    if set(links) - {'KeyMoveNext', 'ViewGroup', 'GroupClipping'}:
        raise ValueError(f"{obj['Id']}: unsupported links {sorted(links)}")
    keys = [obj]
    while keys[-1].get('Links', {}).get('KeyMoveNext'):
        keys.append(keys[-1]['Links']['KeyMoveNext'][0])
    for key in keys:
        if (any(key['Rotate'][a] != 0 for a in 'XYZ') or key.get('InterpolateType', 0) != 0
                or key.get('ActionName') or key.get('KeyHitReactionName')):
            raise ValueError(f"{key['Id']}: only unrotated linear keys without actions are implemented")
    return keys


def sub(a, b):
    return [f32(x - y) for x, y in zip(a, b)]


def dot(a, b):
    return f32(f32(f32(a[0] * b[0]) + f32(a[1] * b[1])) + f32(a[2] * b[2]))


def step_count(keys):
    """ConveyerKeyKeeper::init and ConveyerMapParts::init: total distance, steps and the wrap length."""
    origin, direction = vec(keys[0]['Translate']), [0., 0., 1.]
    distance = [dot(sub(vec(k['Translate']), origin), direction) for k in keys]
    total = 0.
    for a, b in zip(distance, distance[1:]):
        total = f32(total + abs(f32(b - a)))
    interval = max(f32(keys[0].get('PartsInterval', 200.)), 10.)
    return total, interval, int(f32(total / interval)) + 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('geometry', type=Path)
    parser.add_argument('--romfs', type=Path, required=True)
    parser.add_argument('--id', required=True, dest='host_id')
    args = parser.parse_args()
    spec = importlib.util.spec_from_file_location('outfits', Path(__file__).with_name('convert-outfits.py'))
    outfits = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(outfits)
    geometry = json.loads(args.geometry.read_text(encoding='utf8'))
    stage = geometry['stage']
    files = outfits.archive(args.romfs / 'StageData' / (stage + 'Map.szs'))
    placement = outfits.byml(files[stage + 'Map.byml'])[geometry['scenario'] - 1]
    obj = next(o for o in placement.get('ObjectList', []) if o['Id'] == args.host_id)
    keys = conveyor(obj)
    index = next(i for i, o in enumerate(geometry['objects']) if o['id'] == args.host_id)
    host = geometry['objects'][index]
    if host['model'] != obj['UnitConfigName'] or host['translate'] != [obj['Translate'][a] for a in 'XYZ']:
        raise ValueError(f'{args.host_id}: the geometry export does not hold this placement')
    if not geometry['models'].get(host['model'] + '#0', {}).get('prisms'):
        raise ValueError(f'{args.host_id}: the raw KCL prisms of the part are missing')
    total, interval, count = step_count(keys)
    speed, start_rate = f32(obj.get('MoveSpeed', 5.)), f32(obj.get('StartRate', 0.))
    # ConveyerMapParts::init: mOffsetCoord = wrapValue(interval * startRate, interval); step i starts at
    # setTransAndResetByCoord(i * interval + offset); the supported keys are collinear along +Z.
    start = [t for t in host['translate']]
    triangles = [t for t in geometry['triangles'] if t[13] == index]
    others = [t for t in geometry['triangles'] if t[13] != index]
    objects = geometry['objects'][:index] + geometry['objects'][index + 1:]
    # the host object leaves the list: later objects move up by one
    others = [t[:13] + [t[13] - 1 if t[13] > index else t[13]] + t[14:] for t in others]
    new_objects, new_triangles = [], []
    first = len(objects)
    offset = f32((f32(interval * start_rate) + interval) % interval)
    steps = []
    for i in range(count):
        wrap = f32(interval * count)
        coord = f32((f32(f32(f32(interval * i) + offset) + wrap)) % wrap)
        distance = f32(keys[-1]['Translate']['Z'] - keys[0]['Translate']['Z'])
        t = min(max(f32(coord / total), 0.), 1.) if coord < total else 1.
        z = f32(f32(f32(distance * t) + start[2]))
        if coord <= 0:
            z = start[2]
        position = [start[0], start[1], z]
        steps.append(position)
        step = copy.deepcopy(host)
        step['id'] = f'{args.host_id}/{i}'
        step['translate'] = position
        step['static'] = False
        step['matrix'][3], step['matrix'][7], step['matrix'][11] = position
        if step.get('inverse'):
            step['inverse'][3], step['inverse'][7], step['inverse'][11] = -position[0], -position[1], -position[2]
        new_objects.append(step)
        delta = [position[k] - start[k] for k in range(3)]
        for tri in triangles:
            moved = list(tri)
            for corner in range(3):
                for k in range(3):
                    moved[corner * 3 + k] = tri[corner * 3 + k] + delta[k]
            moved[13] = first + i
            new_triangles.append(moved)
    geometry['objects'] = objects + new_objects
    geometry['triangles'] = others + new_triangles
    geometry['conveyor_parts'] = [{
        'id': args.host_id, 'class': 'ConveyerMapParts', 'model': host['model'], 'host': STEP_HOST,
        'speed': speed, 'interval': interval, 'start_rate': start_rate, 'steps': count,
        'keys': [[k['Translate'][a] for a in 'XYZ'] for k in keys], 'total': total,
    }]
    args.geometry.write_text(json.dumps(geometry, separators=(',', ':')), encoding='utf8')
    print(f'{args.geometry}: {count} conveyor steps of {args.host_id}, total {total}, interval {interval}')


if __name__ == '__main__':
    main()
