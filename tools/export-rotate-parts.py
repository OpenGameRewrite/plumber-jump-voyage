"""Add RotateMapParts placements (rotating collision parts) to an existing raw-KCL geometry export.

Only the placements named with --id are exported, and only if their arguments are the ones the
rewrite reproduces: an unrotated unit-scale placement that rotates about its local X or Y axis
(RotateAxis 0 or 1) from the start, with no stage-switch link, no timer, no appear switch and no
link other than the ViewGroup (a draw/camera grouping; a GroupClipping link would tie the part's
clipping to other actors).
The recorded actor CSV is never an input here: the replay seeds the source from it at run time
and tools/rotate_parts.py re-simulates the whole timeline from the placement to check it.
Back up the geometry locally before running.

    py tools/export-rotate-parts.py geometry/LavaWorldClockExStage.json --romfs <romfs> --id obj312
"""
import argparse
import importlib.util
import json
from pathlib import Path

# RotateMapParts::init (al/Library/MapObj/RotateMapParts.cpp) reads RotateAxis and RotateSpeed; a
# listened-to start switch would hold the part in StandBy, which the source model does not have.
SWITCH_KEYS = ('SwitchStart', 'SwitchAppear', 'SwitchA', 'SwitchB', 'SwitchStop')


def rotate_part(obj, part):
    config = obj['UnitConfig']
    if config.get('ParameterConfigName') != 'RotateMapParts':
        raise ValueError(f"{obj['Id']} is not a RotateMapParts")
    if part is None or part.get('model') != obj['UnitConfigName'] or part.get('static') is not False:
        raise ValueError(f"{obj['Id']}: the raw KCL of the part must be in the geometry export")
    if set(obj.get('Links', {})) - {'ViewGroup'} or any(key in obj for key in SWITCH_KEYS):
        raise ValueError(f"{obj['Id']}: switch and link controlled rotors are not implemented")
    axis = obj.get('RotateAxis', 0)
    if axis not in (0, 1):
        raise ValueError(f"{obj['Id']}: only RotateAxis 0 and 1 are implemented")
    if (any(obj['Rotate'][a] != 0 or obj['Scale'][a] != 1 for a in 'XYZ')
            or part['rotate'] != [0, 0, 0] or part['scale'] != [1, 1, 1]
            or part['translate'] != [obj['Translate'][a] for a in 'XYZ']):
        raise ValueError(f"{obj['Id']}: only unrotated unit-scale placements are implemented")
    return {'id': obj['Id'], 'model': obj['UnitConfigName'], 'axis': axis, 'speed': obj.get('RotateSpeed', 100.0),
            'host': config['DisplayName'], 'class': 'RotateMapParts'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('geometry', type=Path)
    parser.add_argument('--romfs', type=Path, required=True)
    parser.add_argument('--id', action='append', required=True, dest='ids')
    args = parser.parse_args()
    spec = importlib.util.spec_from_file_location('outfits', Path(__file__).with_name('convert-outfits.py'))
    outfits = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(outfits)
    geometry = json.loads(args.geometry.read_text(encoding='utf8'))
    stage = geometry['stage']
    files = outfits.archive(args.romfs / 'StageData' / (stage + 'Map.szs'))
    placement = outfits.byml(files[stage + 'Map.byml'])[geometry['scenario'] - 1]
    parts = []
    for obj in placement.get('ObjectList', []):
        if obj['Id'] in args.ids:
            part = next((p for p in geometry['objects'] if p['id'] == obj['Id']), None)
            parts.append(rotate_part(obj, part))
    missing = set(args.ids) - {p['id'] for p in parts}
    if missing:
        raise ValueError(f'placements not found: {sorted(missing)}')
    for part in parts:
        if not geometry['models'].get(part['model'] + '#0', {}).get('prisms'):
            raise ValueError(f"{part['id']}: the raw KCL prisms of the part are missing")
    geometry['rotate_parts'] = parts
    args.geometry.write_text(json.dumps(geometry, separators=(',', ':')), encoding='utf8')
    print(f'{args.geometry}: {len(parts)} rotate parts')


if __name__ == '__main__':
    main()
