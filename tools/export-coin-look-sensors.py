"""Append static placed Coin eye targets from the user's stage to local geometry.

Coin::init/Wait retain placement translation; Coin's Body sensor is on its
origin joint. Only ordinary, unlinked, unattached Coin placements are exported.
This supplies look-at geometry, never collision/damage/contact exemptions.
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
    geometry = json.loads(args.geometry.read_text(encoding='utf-8'))
    stage = geometry['stage']
    files = outfits.archive(args.romfs / 'StageData' / (stage + 'Map.szs'))
    placement = outfits.byml(files[stage + 'Map.byml'])[geometry['scenario'] - 1]
    sensors = geometry.setdefault('look_sensors', [])
    sensors[:] = [s for s in sensors if s.get('source') != 'placed_coin']
    coin = outfits.archive(args.romfs / 'ObjectData' / 'Coin.szs')
    body = next(s for s in outfits.byml(coin['InitSensor.byml']) if s['Name'] == 'Body')
    if body['Joint'] != 'Coin' or body['Type'] != 'MapObj':
        raise ValueError('Coin sensor schema changed')
    for obj in placement.get('ObjectList', []):
        if (obj['UnitConfigName'] != 'Coin' or obj.get('ModelName') != 'Coin'
                or obj.get('Links') or obj.get('IsConnectToCollision')
                or any(obj['Rotate'][a] != 0 or obj['Scale'][a] != 1 for a in 'XYZ')):
            continue
        # Coin::init calls tryAddDisplayOffset; these unrotated placements use
        # the authored UnitConfig offset. Coin's joint itself has no translation.
        offset = obj['UnitConfig'].get('DisplayTranslate', {})
        sensors.append(dict(source='placed_coin', id=obj['Id'], host='コイン', name='Body', type=12,
                            radius=body['Radius'], position=[obj['Translate'][a] + offset.get(a, 0) for a in 'XYZ']))
    args.geometry.write_text(json.dumps(geometry, separators=(',', ':')), encoding='utf-8')
    print(f"{args.geometry}: {len(sensors)} eye targets")


if __name__ == '__main__':
    main()
