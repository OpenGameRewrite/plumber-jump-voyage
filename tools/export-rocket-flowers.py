"""Install recorded RocketFlower placement evidence and initial actor-only seeds locally.

The nearby export must contain original placement and InitSensor records. No player
row is read. Back up the geometry before calling this tool.
"""
import argparse
import csv
import json
from pathlib import Path


def export(geometry, nearby, phase, identity):
    root = json.loads(geometry.read_text(encoding='utf-8'))
    placements = json.loads(nearby.read_text(encoding='utf-8'))
    if (root['stage'], root['scenario']) != (placements['stage'], placements['scenario']):
        raise ValueError('placement stage/scenario mismatch')
    source = next(p for p in placements['objects'] if p['placement']['Id'] == identity)
    p = source['placement']
    if (p['UnitConfigName'] != 'RocketFlower' or p['IsLinkDest']
            or p['Scale'] != dict(X=1., Y=1., Z=1.) or p['Rotate']['X'] != 0 or p['Rotate']['Z'] != 0
            or source['sensors'] != [dict(Name='Body', Radius=70., Type='MapObj', Y=50.),
                                    dict(Name='Equip', Radius=70., Type='Npc')]):
        raise ValueError('unsupported RocketFlower placement/sensors')
    position = [p['Translate'][axis] for axis in 'XYZ']
    source = dict(id=identity, host=p['UnitConfig']['DisplayName'], position=position,
                  placement=p, sensors=source['sensors'])
    actors = list(csv.DictReader(phase.open(encoding='utf-8')))
    first = min(int(r['frame']) for r in actors)
    rows = [r for r in actors if r['id'] == identity and int(r['frame']) == first]
    watcher = phase.with_name('game.rocket_watcher.csv')
    watch = next(r for r in csv.DictReader(watcher.open(encoding='utf-8')) if int(r['frame']) == first)
    if (len(rows) != 2 or any(r['phase'] != 'Wait' for r in rows)
            or watch['phase'] != 'Wait' or watch['count'] != '0'):
        raise ValueError('unsupported initial actor state')
    seed = dict(stage=root['stage'], scenario=root['scenario'], frame=first,
                watcher_step=int(watch['step']), watcher=watch,
                flowers=[dict(id=identity, host=source['host'], position=position,
                              step=int(rows[0]['step']), sensors=rows)])
    root['rocket_flowers'] = [source]
    geometry.write_text(json.dumps(root, ensure_ascii=False, indent=2), encoding='utf-8')
    phase.with_name('game.rocket_seed.json').write_text(json.dumps(seed, ensure_ascii=False, indent=2), encoding='utf-8')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('geometry', type=Path)
    parser.add_argument('--nearby', type=Path, required=True)
    parser.add_argument('--phase', type=Path, required=True)
    parser.add_argument('--id', required=True)
    args = parser.parse_args()
    export(args.geometry, args.nearby, args.phase, args.id)
