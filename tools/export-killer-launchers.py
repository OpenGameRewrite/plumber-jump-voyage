"""Add placed KillerLauncher actors and the switch areas that start them to a geometry export.

    py tools/export-killer-launchers.py geometry/SandWorldHomeStage.s1.json --romfs <romfs> [--out <file>]

A `KillerLauncher` with a `SwitchStart` link starts stopped (`KillerLauncher::init`, 1.0.0 0x147e5c:
`al::listenStageSwitchOnOffStart` answers true for a launcher that has the link), and the link's stage
switch turning on starts it (`startLaunch` 0x148138), turning off ends it (`endLaunch` 0x14819c). The
placement reads `WaitFrame`, `AliveFrame` and `DelayFrame` (`init` 0x147ff0, 0x148014, 0x148034).
A `SwitchKeepOnArea` keeps the switch its `SwitchAreaOn` link names on while the player stands in it
(`al::SwitchKeepOnAreaGroup::update`, `OnCondition` 0: any player position in the volume).

Only the launchers the rewrite reproduces are exported: the plain `KillerLauncher` (not the magnum one)
with a `SwitchStart` link whose switch is driven by at least one `SwitchKeepOnArea` with `OnCondition`
0 and no other launcher link (no kill switch, no `IsSuddenlyLaunchFirst`). A launcher without such a
driver would never start in the replay, and is left out. The poses are the geometry export's own
(`objects` and `areas` carry the matrices), so no rotation arithmetic is repeated here.
The recorded actor CSVs are never an input.

`IsNoCap` must be true (every launcher of the Sand stage sets it): the rewrite models the Killer without
the cap capture, and the flag only changes that and the Killer's effects (see the header of
`src/movement/killer.rs`), not its flight. A launcher that leaves it false is refused, not exported.
"""
import argparse
import importlib.util
import json
from pathlib import Path


def placements(world, scenario):
    return world[scenario - 1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('geometry', type=Path)
    parser.add_argument('--romfs', type=Path, required=True)
    parser.add_argument('--out', type=Path, help='write here instead of over the geometry export')
    args = parser.parse_args()
    spec = importlib.util.spec_from_file_location('outfits', Path(__file__).with_name('convert-outfits.py'))
    outfits = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(outfits)
    geometry = json.loads(args.geometry.read_text(encoding='utf8'))
    stage = geometry['stage']
    world = outfits.byml(outfits.archive(args.romfs / 'StageData' / (stage + 'Map.szs'))[stage + 'Map.byml'])
    scene = placements(world, geometry['scenario'])
    pose = {o['id']: o for o in geometry['objects']}
    area_pose = {a['id']: a for a in geometry['areas']}

    areas = []
    for area in scene.get('AreaList', []):
        if area['UnitConfigName'] != 'SwitchKeepOnArea':
            continue
        links = area.get('Links', {}).get('SwitchAreaOn', [])
        if len(links) != 1 or int(area.get('OnCondition', 0)) != 0 or area['Id'] not in area_pose:
            continue
        exported = area_pose[area['Id']]
        areas.append({
            'id': area['Id'],
            'name': 'SwitchKeepOnArea',
            'shape': exported['shape'],
            'matrix': exported['matrix'],
            'scale': exported['scale'],
            'on_condition': 0,
            'switch': links[0]['Id'],
        })
    driven = {a['switch'] for a in areas}

    launchers = []
    for obj in scene.get('ObjectList', []):
        # Only the plain launcher is accepted, on purpose. `KillerLauncher::init` (0x147e5c) picks the
        # Killer's kind from the actor's class name: `KillerLauncherMagnum` sets the byte at launcher
        # +0x12d (0x147ec0), copied to Killer +0x136 (0x147f84), and that kind indexes the Killer's
        # flight constants (the acceleration, damping, turn and distance tables from 0x1886940) and
        # the fly threshold of `Killer::exeLaunch` (25 plain at 0x146860, 40 Magnum at 0x146864,
        # selected at 0x146868). The rewrite ports kind 0 only, so every other
        # `UnitConfigName` (the Magnum launcher and the "Dot" variants) is skipped here, never exported
        # with the plain constants.
        if obj['UnitConfigName'] != 'KillerLauncher' or obj['Id'] not in pose:
            continue
        links = obj.get('Links', {})
        start = links.get('SwitchStart', [])
        if len(start) != 1 or start[0]['Id'] not in driven:
            continue
        if set(links) - {'SwitchStart', 'ViewGroup'} or obj.get('IsSuddenlyLaunchFirst'):
            raise ValueError(f"{obj['Id']}: this launcher configuration is not implemented")
        # `IsNoCap` (read at 0x147ed8, copied to Killer +0x137 at 0x147f90) is modelled as true.
        if obj.get('IsNoCap') is not True:
            raise ValueError(f"{obj['Id']}: IsNoCap must be true, the cap capture is not implemented")
        launchers.append({
            'id': obj['Id'],
            'class': 'KillerLauncher',
            'matrix': pose[obj['Id']]['matrix'],
            'wait_frame': int(obj.get('WaitFrame', 120)),  # init fallback at 0x147ff8
            'alive_frame': int(obj.get('AliveFrame', 300)),  # init fallback at 0x14801c
            'delay_frame': int(obj.get('DelayFrame', -1)),  # init fallback at 0x14805c
            'switch': start[0]['Id'],
        })
    used = {launcher['switch'] for launcher in launchers}
    geometry['killer_launchers'] = launchers
    geometry['switch_areas'] = [a for a in areas if a['switch'] in used]
    out = args.out or args.geometry
    out.write_text(json.dumps(geometry, separators=(',', ':')), encoding='utf8')
    print(f"{out}: {len(launchers)} killer launcher(s), {len(geometry['switch_areas'])} switch area(s)")


if __name__ == '__main__':
    main()
