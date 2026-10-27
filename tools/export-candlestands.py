"""Export settled, initially lit candlestands from local RomFS into a stage mesh.

Only upright, unit-scale, unattached LavaWorldCandlestand000 placements with no
state/appearance links are supported. ViewGroup links only affect visibility.
The replay fixture requires the recorder's 120-update stage settle; appear()
disables clipping until BurnUp 180, and Fire/Body are valid by BurnUp 60.
Back up existing geometry before running this additive tool.
"""
import argparse
import importlib.util
import json
from pathlib import Path
import struct


def f32(x):
    return struct.unpack('<f', struct.pack('<f', x))[0]


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
    model = 'LavaWorldCandlestand000'
    files = outfits.archive(args.romfs / 'ObjectData' / (model + '.szs'))
    sensors = {s['Name']: s for s in outfits.byml(files['InitSensor.byml'])}
    for name, kind, radius, y in [('Fire', 'EnemyAttack', 40., 200.), ('Body', 'MapObj', 80., 210.)]:
        sensor = sensors[name]
        if (sensor['Type'], sensor['Radius'], sensor['Y']) != (kind, radius, y):
            raise ValueError('Candlestand sensor schema changed')
        if any(sensor.get(k, 0) for k in ('X', 'Z', 'Joint')):
            raise ValueError('Unsupported sensor transform')
    candles = []
    looks = geometry.setdefault('look_sensors', [])
    looks[:] = [s for s in looks if s.get('source') != 'candlestand']
    for obj in placement.get('ObjectList', []):
        if (obj['UnitConfigName'] != model or obj.get('IsBurnOut') is not False
                or obj.get('IsConnectToCollision') is not False or obj.get('IsFireRecover')
                or set(obj.get('Links', {})) - {'ViewGroup'}
                or any(obj['Rotate'][a] != 0 or obj['Scale'][a] != 1 for a in 'XYZ')):
            continue
        part = next((p for p in geometry['objects'] if p['id'] == obj['Id'] and p['model'] == model), None)
        if part is None:
            continue
        position = [obj['Translate'][a] for a in 'XYZ']
        if position != part['translate']:
            raise ValueError('Placement and collision export disagree')
        part['static'] = True
        host = obj['UnitConfig']['DisplayName']
        fire = [position[0], f32(position[1] + sensors['Fire']['Y']), position[2]]
        body = [position[0], f32(position[1] + sensors['Body']['Y']), position[2]]
        candles.append(dict(id=obj['Id'], host=host, is_burn_out=False, settled=True,
                            fire_position=fire, body_position=body))
        # Only Body is a look-at candidate; Fire is EnemyAttack, Wick is invalid.
        looks.append(dict(source='candlestand', id=obj['Id'], host=host, name='Body',
                          type=12, radius=80., position=body))
    geometry['candlestands'] = candles
    args.geometry.write_text(json.dumps(geometry, separators=(',', ':')), encoding='utf8')
    print(f'{args.geometry}: {len(candles)} settled candlestands')


if __name__ == '__main__':
    main()
