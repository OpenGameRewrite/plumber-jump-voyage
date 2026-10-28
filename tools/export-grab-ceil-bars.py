"""Export fixed, unconnected PoleGrabCeil sources and independent preroll seeds.

Requires the user's extracted RomFS. Back up geometry before running this tool.
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
    parser.add_argument('--phase', type=Path, required=True)
    args = parser.parse_args()
    spec = importlib.util.spec_from_file_location('outfits', Path(__file__).with_name('convert-outfits.py'))
    outfits = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(outfits)
    geometry = json.loads(args.geometry.read_text(encoding='utf8'))
    stage = geometry['stage']
    archive = outfits.archive(args.romfs / 'StageData' / (stage + 'Map.szs'))
    placement = outfits.byml(archive[stage + 'Map.byml'])[geometry['scenario'] - 1]
    archive = outfits.archive(args.romfs / 'ObjectData/PoleGrabCeil.szs')
    sensors = outfits.byml(archive['InitSensor.byml'])
    snap, = [s for s in sensors if s['Name'] == 'Snap']
    if snap != dict(Name='Snap', Radius=150., Type='MapObj'):
        raise ValueError('Unsupported Snap schema')
    if any(a['UnitConfigName'] == 'PoleGrabCeilNoSnapArea' for a in placement.get('AreaList', [])):
        raise ValueError('NoSnap areas are not implemented')
    sources = []
    for obj in placement.get('ObjectList', []):
        if (obj['UnitConfigName'] != 'PoleGrabCeil'
                or obj.get('IsConnectToCollision') is not False
                or obj.get('IsConnectPose') is not False or obj.get('Links')
                or any(obj['Rotate'][a] != 0 or obj['Scale'][a] != 1 for a in 'XYZ')):
            continue
        part = next((p for p in geometry['objects'] if p['id'] == obj['Id']), None)
        if part is None:
            continue
        part['static'] = True
        sources.append(dict(id=obj['Id'], host=obj['UnitConfig']['DisplayName'],
                            position=[obj['Translate'][a] for a in 'XYZ'], side=[1., 0., 0.],
                            radius=150., sensor='Snap', type=12))
    rows = list(csv.DictReader(args.phase.open(encoding='utf8')))
    frame = min(int(r['frame']) for r in rows)
    seeds = [r for r in rows if int(r['frame']) == frame and any(r['id'] == s['id'] for s in sources)]
    for source in sources:
        seed, = [r for r in seeds if r['id'] == source['id']]
        if ([float(seed['pos_' + a]) for a in 'xyz'] != source['position']
                or seed['phase'] != 'Wait' or seed['n'] != '1d1f390'
                or seed['radius_bits'] != '43160000'
                or any(seed[k] != '0' for k in ('dead', 'registry_dropped'))
                or any(seed[k] != '1' for k in ('sensor', 'valid'))):
            raise ValueError('Unsupported independent bar seed')
    # CoinRing only supplies look-at observations; no physical coin exemption.
    looks = geometry.setdefault('look_sensors', [])
    looks[:] = [s for s in looks if s.get('source') != 'stationary_coin_ring']
    for obj in placement.get('ObjectList', []):
        if (obj['UnitConfigName'] == 'CoinRing' and obj.get('MoveType') == 0
                and not obj.get('Links') and obj.get('IsConnectToCollision') is False
                and all(obj['Rotate'][a] == 0 and obj['Scale'][a] == 1 for a in 'XYZ')):
            looks.append(dict(source='stationary_coin_ring', id=obj['Id'], host=obj['UnitConfig']['DisplayName'],
                              name='Body', type=12, radius=140., position=[obj['Translate'][a] for a in 'XYZ']))
    geometry['grab_ceil_bars'] = sources
    args.geometry.write_text(json.dumps(geometry, separators=(',', ':')), encoding='utf8')
    seed_path = args.phase.with_name('game.grabceil_seed.json')
    seed_path.write_text(json.dumps(dict(stage=stage, scenario=geometry['scenario'], frame=frame,
                                         sources=seeds), indent=2), encoding='utf8')
    print(f'{len(sources)} fixed bars; seed {seed_path}')


if __name__ == '__main__':
    main()
