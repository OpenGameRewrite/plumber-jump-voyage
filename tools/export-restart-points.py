"""Add restart placements from the user's extracted StageData to a local geometry export."""
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
    placement = outfits.byml(files[stage + 'Map.byml'])[geometry['scenario'] - 1]

    def point(obj, kind):
        return dict(id=obj['Id'], kind=kind,
                    position=[obj['Translate'][axis] for axis in 'XYZ'],
                    rotate=[obj['Rotate'][axis] for axis in 'XYZ'])

    points = [point(obj, 'stage') for obj in placement.get('PlayerList', [])
              if obj['UnitConfigName'] == 'PlayerActorHakoniwa']
    for flag in placement.get('CheckPointList', []):
        for linked in flag.get('Links', {}).get('PlayerRestartPos', []):
            restart = point(linked, 'checkpoint')
            restart['checkpoint_id'] = flag['Id']
            points.append(restart)
    geometry['restart_points'] = points
    args.geometry.write_text(json.dumps(geometry, separators=(',', ':')), encoding='utf8')
    print(f'{args.geometry}: {len(points)} restart placements')


if __name__ == '__main__':
    main()
