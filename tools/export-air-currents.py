"""Add placed AirCurrent wind sources to an existing geometry export.

AirCurrent::init (1.0.0 0x212b90) reads WaitFrame, BlowFrame, DelayFrame and IsForceCalc from the placement and
takes the blow length from the local Z scale; its pose is the placement's translate and rotate. The stock
exporter reads a stage's own ObjectList only, so a source that lives in a zone placement (a ZoneList entry of
the stage) is composed with the zone's translate, the way the zone is loaded in game. Only the sources the
rewrite reproduces are accepted: no stage-switch or other links (a switched source is not exported at all:
the recording's own actor rows show it asleep or stopped), a single key pose (the source does not move), an
identity zone rotation and unit zone scale.
The recorded actor CSV is never an input here: tools/air_current.py re-simulates the nerve chain from the
placement and compares it with the recording.

    py tools/export-air-currents.py geometry/SnowWorldTownStage.s1.json --romfs <romfs> \
        --zone SnowWorldBalconyZone --id obj265
"""
import argparse
import importlib.util
import json
import struct
from pathlib import Path


def f32(x):
    return struct.unpack('<f', struct.pack('<f', x))[0]


def air_current(obj, zone_placement):
    config = obj['UnitConfig']
    if config.get('ParameterConfigName') != 'AirCurrent':
        raise ValueError(f"{obj['Id']} is not an AirCurrent")
    if obj.get('Links'):
        raise ValueError(f"{obj['Id']}: linked (switched or key-moved) sources are not implemented")
    if (any(zone_placement['Rotate'][a] != 0 for a in 'XYZ')
            or any(zone_placement['Scale'][a] != 1 for a in 'XYZ')):
        raise ValueError('only identity-rotation unit-scale zones are implemented')
    translate = [f32(obj['Translate'][a] + zone_placement['Translate'][a]) for a in 'XYZ']
    return {
        'id': obj['Id'],
        'class': 'AirCurrent',
        'zone': zone_placement['Id'],
        'zone_name': zone_placement['UnitConfigName'],
        'translate': translate,
        'rotate': [f32(obj['Rotate'][a]) for a in 'XYZ'],
        'scale': [f32(obj['Scale'][a]) for a in 'XYZ'],
        'wait_frame': int(obj.get('WaitFrame', 0)),
        'blow_frame': int(obj.get('BlowFrame', -1)),
        'delay_frame': int(obj.get('DelayFrame', 0)),
        'force_calc': bool(obj.get('IsForceCalc', False)),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('geometry', type=Path)
    parser.add_argument('--romfs', type=Path, required=True)
    parser.add_argument('--zone', required=True, help='the zone placement file (UnitConfigName in ZoneList)')
    parser.add_argument('--id', required=True, action='append', dest='ids')
    args = parser.parse_args()
    spec = importlib.util.spec_from_file_location('outfits', Path(__file__).with_name('convert-outfits.py'))
    outfits = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(outfits)
    geometry = json.loads(args.geometry.read_text(encoding='utf8'))
    stage = geometry['stage']
    world = outfits.byml(outfits.archive(args.romfs / 'StageData' / (stage + 'Map.szs'))[stage + 'Map.byml'])
    zone_placement = next(p for p in world[geometry['scenario'] - 1]['ZoneList'] if p['UnitConfigName'] == args.zone)
    zone = outfits.byml(outfits.archive(args.romfs / 'StageData' / (args.zone + 'Map.szs'))[args.zone + 'Map.byml'])
    objects = {o['Id']: o for o in zone[0].get('ObjectList', [])}
    geometry['air_currents'] = [air_current(objects[i], zone_placement) for i in args.ids]
    args.geometry.write_text(json.dumps(geometry, separators=(',', ':')), encoding='utf8')
    print(f"{args.geometry}: {len(args.ids)} air current(s) of {args.zone}")


if __name__ == '__main__':
    main()
