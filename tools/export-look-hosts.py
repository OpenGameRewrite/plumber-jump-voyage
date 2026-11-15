"""Add placed BlockBrick/BlockQuestion/NoteObjFirst head look-at sensors and the camera clipping of the look hosts.

    py tools/export-look-hosts.py <geometry.json> --romfs <romfs>

The head look-at (`src/movement/look_at.rs`) picks the nearest valid sensor in the eye's reach. Two
facts of the user's own stage decide which sensors that is, and neither is in the static collision:

* Placed `BlockBrick` and `BlockQuestion` actors offer their `MapObj` sensors (`InitSensor.byml`:
  Body and BlockUpperPunch, offset above the actor). `BlockQuestion::receiveMsg` answers neither
  `PlayerDisregard` nor `PlayerLookAtPosition`; `BlockBrick` has no decomp source, so that it does
  not disregard the player is not proven here.
* Actors outside the camera frustum have their sensors invalid. Each look host's `InitClipping`
  radius and its placement centre are exported as a `look_clipping` sphere tied to the placement
  `id`, for the blocks and for the stage's `CoinCollect` placements (the empties the save leaves).

* A placed `NoteObjFirst` (the first note of a note chain) offers its `MapObj` Body sensor too:
  `NoteObj::receiveMsg` (0x2d0c0c) answers the target-marker, item, cap, Yoshi and weapon
  messages only, not `PlayerDisregard` or `PlayerLookAtPosition`. `NoteObj::init` (0x2d07e8)
  moves the actor by the placement's `DisplayTranslate` (`al::tryAddDisplayOffset`), and with a
  `Start` link widens the clipping sphere to the placement's `ShadowLength` (default 1500, 10000
  for `IsLongShadow`; `al::expandClippingRadiusByShadowLength` with no offset takes the larger of
  the two radii and keeps the centre).

Only unlinked, unrotated (about X and Z), unit-scale placements named after their class are
exported (a note keeps its `ShineActor`, `Start` and `ViewGroup` links). Running it again
replaces its own entries. `NearClipDistance`, group and far clipping are not exported: the
camera-side planes are all `replay::update_static_clipping` knows.
"""
import argparse
import importlib.util
import json
import struct
from pathlib import Path

SOURCE = 'look_host'
SENSOR_TYPES = {'MapObj': 12}
# class -> (the name the game logs for the actor, its object archive)
BLOCKS = {'BlockBrick': ('レンガブロック', 'BlockBrick'), 'BlockQuestion': ('ハテナブロック', 'BlockQuestion')}
COIN_HOST = 'コレクトコイン空'
NOTE_FIRST = 'NoteObjFirst'
SHADOW_LENGTH = 1500.
LONG_SHADOW_LENGTH = 10000.


def f32(value):
    return struct.unpack('f', struct.pack('f', value))[0]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('geometry', type=Path)
    parser.add_argument('--romfs', type=Path, required=True)
    args = parser.parse_args()
    spec = importlib.util.spec_from_file_location('outfits', Path(__file__).with_name('convert-outfits.py'))
    outfits = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(outfits)
    geometry = json.loads(args.geometry.read_text(encoding='utf-8'))
    files = outfits.archive(args.romfs / 'StageData' / (geometry['stage'] + 'Map.szs'))
    placements = outfits.byml(files[geometry['stage'] + 'Map.byml'])[geometry['scenario'] - 1]
    looks = geometry.setdefault('look_sensors', [])
    clips = geometry.setdefault('look_clipping', [])
    looks[:] = [s for s in looks if s.get('source') != SOURCE]
    clips[:] = [c for c in clips if c.get('source') != SOURCE]

    def placed(obj, name):
        return (obj['UnitConfigName'] == name and obj.get('ModelName') == name
                and not obj.get('Links') and not obj.get('IsLinkDest')
                and all(obj['Rotate'][a] == 0 for a in 'XZ') and all(obj['Scale'][a] == 1 for a in 'XYZ'))

    for name, (host, archive_name) in BLOCKS.items():
        archive = outfits.archive(args.romfs / 'ObjectData' / (archive_name + '.szs'))
        sensors = [s for s in outfits.byml(archive['InitSensor.byml']) if s['Type'] == 'MapObj']
        radius = outfits.byml(archive['InitClipping.byml'])['Radius']
        for obj in placements.get('ObjectList', []):
            if not placed(obj, name):
                continue
            position = [obj['Translate'][a] for a in 'XYZ']
            for sensor in sensors:
                looks.append(dict(source=SOURCE, id=obj['Id'], **{'class': name}, host=host, name=sensor['Name'],
                                  type=SENSOR_TYPES[sensor['Type']], radius=sensor['Radius'],
                                  position=[position[0], f32(position[1] + sensor.get('Y', 0.)), position[2]]))
            clips.append(dict(source=SOURCE, id=obj['Id'], host=host, position=position, radius=radius))

    archive = outfits.archive(args.romfs / 'ObjectData' / (NOTE_FIRST + '.szs'))
    sensors = [s for s in outfits.byml(archive['InitSensor.byml']) if s['Type'] == 'MapObj']
    note_radius = outfits.byml(archive['InitClipping.byml'])['Radius']
    for obj in placements.get('ObjectList', []):
        if (obj['UnitConfigName'] != NOTE_FIRST or obj.get('ModelName') is not None
                or set(obj.get('Links', {})) - {'ShineActor', 'Start', 'ViewGroup'}
                or any(obj['Rotate'][a] != 0 for a in 'XZ') or any(obj['Scale'][a] != 1 for a in 'XYZ')):
            continue
        host = obj['UnitConfig']['DisplayName']
        offset = obj['UnitConfig']['DisplayTranslate']
        position = [f32(obj['Translate'][a] + offset[a]) for a in 'XYZ']
        for sensor in sensors:
            looks.append(dict(source=SOURCE, id=obj['Id'], **{'class': NOTE_FIRST}, host=host, name=sensor['Name'],
                              type=SENSOR_TYPES[sensor['Type']], radius=sensor['Radius'],
                              position=[position[0], f32(position[1] + sensor.get('Y', 0.)), position[2]]))
        radius = note_radius
        if 'Start' in obj.get('Links', {}):
            length = LONG_SHADOW_LENGTH if obj.get('IsLongShadow') else obj.get('ShadowLength', SHADOW_LENGTH)
            radius = max(radius, length)
        clips.append(dict(source=SOURCE, id=obj['Id'], host=host, position=position, radius=radius))

    # The save leaves a CoinCollectEmpty at every collected CoinCollect placement; every world's
    # archive must agree on the clipping radius.
    radii = {outfits.byml(outfits.archive(path)['InitClipping.byml'])['Radius']
             for path in sorted((args.romfs / 'ObjectData').glob('CoinCollectEmpty?.szs'))}
    if len(radii) != 1:
        raise ValueError('CoinCollectEmpty archives disagree on the clipping radius')
    coin_radius = radii.pop()
    for coin in looks:
        if coin.get('class') == 'CoinCollect':
            clips.append(dict(source=SOURCE, id=coin['id'], host=coin['host'], position=coin['position'],
                              radius=coin_radius))
    args.geometry.write_text(json.dumps(geometry, separators=(',', ':'), ensure_ascii=False), encoding='utf-8')
    print(f'{args.geometry}: {len(looks)} look sensors, {len(clips)} clipping spheres')


if __name__ == '__main__':
    main()
