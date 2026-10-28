"""Add stationary FigureWalkingNpc look sensors from the user's own RomFS.

FigureWalkingNpc::init (1.0.0, 0x388e58) loads NokonokoNpc, not an archive
named after its class. This fixture covers the idle NPC before starting its
minigame. Back up the geometry before running this additive export.
"""
import argparse
import importlib.util
import json
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('geometry', type=Path)
    parser.add_argument('--romfs', type=Path, required=True)
    args = parser.parse_args()
    spec = importlib.util.spec_from_file_location('outfits', Path(__file__).with_name('convert-outfits.py'))
    outfits = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(outfits)
    geometry = json.loads(args.geometry.read_text(encoding='utf8'))
    stage = geometry['stage']
    files = outfits.archive(args.romfs / 'StageData' / (stage + 'Map.szs'))
    placements = outfits.byml(files[stage + 'Map.byml'])[geometry['scenario'] - 1]
    npc = outfits.archive(args.romfs / 'ObjectData' / 'NokonokoNpc.szs')
    sensors = outfits.byml(npc['InitSensor.byml'])
    clipping = outfits.byml(npc['InitClipping.byml'])
    if len(sensors) != 1 or sensors[0] != {'Name': 'Body', 'Radius': 80., 'Type': 'Npc', 'Y': 80.}:
        raise ValueError('Unsupported NokonokoNpc sensor schema')
    if clipping != {'Radius': 300.}:
        raise ValueError('Unsupported NokonokoNpc clipping schema')
    host = '\u30d5\u30a3\u30ae\u30e5\u30a2\u30a6\u30a9\u30fc\u30ad\u30f3\u30b0Npc'
    looks = geometry.setdefault('look_sensors', [])
    clips = geometry.setdefault('look_clipping', [])
    looks[:] = [s for s in looks if s.get('class') != 'FigureWalkingNpc']
    clips[:] = [s for s in clips if s.get('host') != host]
    for obj in placements.get('ObjectList', []):
        if obj['UnitConfigName'] != 'FigureWalkingNpc':
            continue
        if (any(obj['Scale'][k] != 1. for k in 'XYZ')
                or any(obj['Rotate'][k] != 0. for k in 'XZ')
                or obj.get('Links', {}).get('Rail')):
            raise ValueError('Only upright unit-scale stationary NPC placements are supported')
        position = [obj['Translate'][k] for k in 'XYZ']
        looks.append({'id': obj['Id'], 'class': 'FigureWalkingNpc', 'host': host,
                      'name': 'Body', 'type': 6, 'radius': sensors[0]['Radius'],
                      'position': [position[0], position[1] + sensors[0]['Y'], position[2]]})
        clips.append({'host': host, 'position': position, 'radius': clipping['Radius']})
    args.geometry.write_text(json.dumps(geometry, separators=(',', ':')), encoding='utf8')
    print(f'{args.geometry}: exported stationary FigureWalkingNpc sensors and clipping')


if __name__ == '__main__':
    main()
