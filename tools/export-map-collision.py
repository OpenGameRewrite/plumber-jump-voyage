"""Annotate RippleFixMapParts collision registration from the user's local archives.

RippleFixMapParts::init (1.0.0, 0x89e4cc) uses initMapPartsActor; initActorImpl
(0x8dca20..0x8dcb50) creates the part only if InitCollision[Suffix].byml exists.
A KCL alone does not create it. Other actor classes may create parts themselves
and are deliberately left unclassified. Existing triangle and object indices stay
intact so runtime prism evidence remains usable.
"""
import argparse
import importlib.util
import json
import shutil
from pathlib import Path


def collision_file(placement):
    return 'InitCollision' + (placement.get('Suffix') or '') + '.byml'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('geometry', type=Path)
    parser.add_argument('--romfs', type=Path, required=True)
    args = parser.parse_args()
    spec = importlib.util.spec_from_file_location('outfits', Path(__file__).with_name('convert-outfits.py'))
    outfits = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(outfits)
    geometry = json.loads(args.geometry.read_text(encoding='utf-8'))
    stage = geometry['stage']
    files = outfits.archive(args.romfs / 'StageData' / (stage + 'Map.szs'))
    placements = outfits.byml(files[stage + 'Map.byml'])[geometry['scenario'] - 1]
    placements = {o['Id']: o for o in placements.get('ObjectList', [])}
    changed = []
    for obj in geometry['objects']:
        source = placements[obj['id']]
        if source.get('UnitConfig', {}).get('ParameterConfigName') != 'RippleFixMapParts':
            continue
        archive = outfits.archive(args.romfs / 'ObjectData' / (obj['model'] + '.szs'))
        obj['collision_enabled'] = collision_file(source) in archive
        changed.append((obj['id'], obj['model'], obj['collision_enabled']))
    backup = args.geometry.parent / 'old' / (args.geometry.stem + '.before-wave10-collision.json')
    backup.parent.mkdir(exist_ok=True)
    if backup.exists():
        raise SystemExit(f'Backup already exists: {backup}; preserve it before rerunning')
    shutil.copy2(args.geometry, backup)
    args.geometry.write_text(json.dumps(geometry, separators=(',', ':')), encoding='utf-8')
    print(changed)


if __name__ == '__main__':
    main()
