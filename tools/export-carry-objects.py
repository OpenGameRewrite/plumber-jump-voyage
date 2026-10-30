"""Add carry placements and original InitSensor data to a local collision export.

Only INITIAL init/used-query records may establish saved availability. Runtime poses,
sensor samples, attachment matrices and message transcripts are never replay input.
"""
import argparse
import csv
import importlib.util
import json
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('geometry', type=Path)
    parser.add_argument('--romfs', type=Path, required=True)
    parser.add_argument('--initial', type=Path, required=True)
    parser.add_argument('--id', action='append', required=True)
    args = parser.parse_args()
    spec = importlib.util.spec_from_file_location('outfits', Path(__file__).with_name('convert-outfits.py'))
    outfits = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(outfits)
    root = json.loads(args.geometry.read_text(encoding='utf8'))
    files = outfits.archive(args.romfs / 'StageData' / (root['stage'] + 'Map.szs'))
    stage = outfits.byml(files[root['stage'] + 'Map.byml'])[root['scenario'] - 1]
    with args.initial.open(encoding='utf8', newline='') as stream:
        # Filter before inspecting pose/state fields: post-INITIAL rows cannot seed anything.
        initial = [r for r in csv.DictReader(stream) if r['phase'] == 'init' and r['kind'] == 'init']
    sources = []
    for parent in stage['ObjectList']:
        if parent['UnitConfigName'] != 'GrowFlowerWatcher':
            continue
        for children in parent.get('Links', {}).values():
            for p in children:
                if p['Id'] not in args.id:
                    continue
                matches = [r for r in initial if r['id'] == p['Id'] and r['parent'] == parent['Id']]
                if len(matches) != 1:
                    raise ValueError('ambiguous/missing INITIAL object: ' + p['Id'])
                row = matches[0]
                if row['pos_values'] != '/'.join(str(float(p['Translate'][a])) for a in 'XYZ'):
                    raise ValueError('INITIAL placement mismatch: ' + p['Id'])
                archive = outfits.archive(args.romfs / 'ObjectData' / (p['UnitConfigName'] + '.szs'))
                sensors = outfits.byml(archive['InitSensor.byml'])
                if p['UnitConfigName'] == 'GrowFlowerPot':
                    passive = [{k: s[k] for k in ('Name', 'Radius', 'Y')} for s in sensors if s['Name'] == 'SeedNear']
                    sensors = [{k: s[k] for k in ('Name', 'Radius', 'Y')} for s in sensors if s['Name'] == 'Body']
                else:
                    passive = []
                sources.append(dict(id=p['Id'], parent=parent['Id'], placement=p, sensors=sensors,
                                    passive_sensors=passive,
                                    initial_alive=row['dead'] == '0', initial_phase='init',
                                    host={'GrowFlowerPot':'[育つ花]植木鉢', 'GrowFlowerSeed':'[育つ花]種'}[p['UnitConfigName']]))
    for p in stage['ObjectList']:
        if p['Id'] not in args.id or p['UnitConfigName'] != 'KickStone':
            continue
        matches = [r for r in initial if r['id'] == p['Id'] and not r['parent']]
        if len(matches) != 1:
            raise ValueError('ambiguous/missing INITIAL stone: ' + p['Id'])
        row = matches[0]
        if list(map(float, row['pos_values'].split('/'))) != [p['Translate'][a] for a in 'XYZ']:
            raise ValueError('INITIAL placement mismatch: ' + p['Id'])
        archive = outfits.archive(args.romfs / 'ObjectData' / 'KickStone.szs')
        sources.append(dict(id=p['Id'], parent='', placement=p,
                            sensors=outfits.byml(archive['InitSensor.byml']),
                            collider=outfits.byml(archive['InitCollider.byml']),
                            initial_alive=row['dead'] == '0', initial_phase='init',
                            initial_joint_quat=list(map(float, row['joint_quat_values'].split('/'))),
                            initial_kick_count=int(row['kick_count']),
                            host=p['UnitConfig']['DisplayName']))
    # The floor is a concrete PaintObj placement. Export its identity and sensor contract;
    # the environment judge still requires the original rejected message at each overlap.
    if root['stage'] == 'ForestWorldHomeStage' and root['scenario'] == 1 and any(s['id'] == 'obj1837' for s in sources):
        p = next(p for p in stage['ObjectList'] if p['Id'] == 'obj7234')
        archive = outfits.archive(args.romfs / 'ObjectData' / (p['UnitConfigName'] + '.szs'))
        root['paint_objects'] = [dict(id=p['Id'], placement=p,
                                      host=p['UnitConfig']['DisplayName'],
                                      sensors=outfits.byml(archive['InitSensor.byml']))]
    if {s['id'] for s in sources} != set(args.id):
        raise ValueError('not every requested linked placement was found')
    root['carry_objects'] = sources
    args.geometry.write_text(json.dumps(root, separators=(',', ':')), encoding='utf8')
    print(f'{args.geometry}: {len(sources)} carry placements')


if __name__ == '__main__':
    main()
