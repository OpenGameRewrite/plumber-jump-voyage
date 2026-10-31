"""Add original Cappy-interaction source placements and their INITIAL phase to a local export.

    py tools/export-cap-sources.py <geometry.json> --romfs <romfs> --initial <game.cap_sources.csv> --id obj454 ...

Each id is a SignBoardNormal (`signboards`), CapHanger (`cap_hangers`) or CapSwitchSave
(`cap_switches`) placement. The export keeps the placement, the object archive's InitSensor
(and InitCollision) data (a switch also its InitHackCap, hit reactions and model curves) and the
pending nerve of the INITIAL record, which is proof that the actor has not executed yet. Runtime samples and message transcripts never initialize a replay.
"""
import argparse
import csv
import importlib.util
import json
import subprocess
import tempfile
from pathlib import Path

# Pending nerve pointers of an actor's INITIAL record (main.bin vtable offsets of the 1.0.0
# executable); a different value means the source already ran or is another class.
INITIAL_NERVE = {'SignBoardNormal': '1d30278', 'CapHanger': '1ce0b60', 'CapSwitchSave': '1ce3e98'}
KEYS = {'SignBoardNormal': 'signboards', 'CapHanger': 'cap_hangers', 'CapSwitchSave': 'cap_switches'}
# `CapSwitch::init` loads the archive named CapSwitch whatever the placement's unit is called.
ARCHIVE = {'CapSwitchSave': 'CapSwitch'}


def model_curves(archive, converter):
    """The actor's skeleton and the curves of all its clips as the converter's curve writer
    stores them ("PJVCURV1", docs/asset-format.md), hex encoded."""
    with tempfile.TemporaryDirectory() as folder:
        model, curves = Path(folder) / 'model.bfres', Path(folder) / 'model.curves.bin'
        model.write_bytes(next(v for k, v in archive.items() if k.endswith('.bfres')))
        subprocess.run([str(converter), str(model), str(curves), str(model), '--curves-only', '--all-animations'],
                       check=True, capture_output=True)
        return curves.read_bytes().hex()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('geometry', type=Path)
    parser.add_argument('--romfs', type=Path, required=True)
    parser.add_argument('--initial', type=Path, required=True)
    parser.add_argument('--id', action='append', required=True)
    parser.add_argument('--converter', type=Path, default=Path('local/converter/convert.exe'))
    args = parser.parse_args()
    spec = importlib.util.spec_from_file_location('outfits', Path(__file__).with_name('convert-outfits.py'))
    outfits = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(outfits)
    root = json.loads(args.geometry.read_text(encoding='utf8'))
    files = outfits.archive(args.romfs / 'StageData' / (root['stage'] + 'Map.szs'))
    stage = outfits.byml(files[root['stage'] + 'Map.byml'])[root['scenario'] - 1]
    with args.initial.open(encoding='utf8', newline='') as stream:
        initial = {r['id']: r for r in csv.DictReader(stream) if r['kind'] == 'init'}
    for key in KEYS.values():
        root.pop(key, None)
    for identity in args.id:
        placement = next(p for p in stage['ObjectList'] if p['Id'] == identity)
        source = placement['UnitConfigName']
        if source not in KEYS:
            raise ValueError(f'{identity}: {source} is not a supported Cappy-interaction source')
        record = initial.get(identity)
        position = [placement['Translate'][a] for a in 'XYZ']
        if (record is None or record['current_n'] != '0' or record['pending_n'] != INITIAL_NERVE[source]
                or record['step'] != '0' or record['dead'] != '0' or record['clipped'] != '0'
                or [float(record[f'pos_{a}']) for a in 'xyz'] != position):
            raise ValueError(f'{identity}: unsupported INITIAL source phase')
        archive = outfits.archive(args.romfs / 'ObjectData' / (ARCHIVE.get(source, source) + '.szs'))
        entry = dict(placement=placement, sensors=outfits.byml(archive['InitSensor.byml']),
                     initial_phase='WaitPending')
        if source == 'CapSwitchSave':
            # The cap follows the switch's HatTarget joint (`InitHackCap.byml`), which its clips animate.
            entry['hack_cap'] = outfits.byml(archive['InitHackCap.byml'])
            entry['hit_reactions'] = outfits.byml(archive['ActorHitReactionCtrl.byml'])
            entry['curves'] = model_curves(archive, args.converter)
        if 'InitCollision.byml' in archive:
            entry['collision'] = outfits.byml(archive['InitCollision.byml'])
        root.setdefault(KEYS[source], []).append(entry)
    args.geometry.write_text(json.dumps(root, ensure_ascii=False, indent=2), encoding='utf8')


if __name__ == '__main__':
    main()
