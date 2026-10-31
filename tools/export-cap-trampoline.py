"""Add an original stationary CapTrampoline placement and INITIAL phase to a local export.

The input collision view must already omit inactive source collision. Runtime
samples and message transcripts are never used to initialize the replay.
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
    parser.add_argument('--id', required=True)
    args = parser.parse_args()
    spec = importlib.util.spec_from_file_location('outfits', Path(__file__).with_name('convert-outfits.py'))
    outfits = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(outfits)
    root = json.loads(args.geometry.read_text(encoding='utf8'))
    files = outfits.archive(args.romfs / 'StageData' / (root['stage'] + 'Map.szs'))
    stage = outfits.byml(files[root['stage'] + 'Map.byml'])[root['scenario']-1]
    placement = next(p for p in stage['ObjectList'] if p['Id'] == args.id)
    if (placement['UnitConfigName'] != 'CapTrampoline'
            or placement['IsConnectToCollision'] or placement['IsLinkDest'] or placement['Links']):
        raise ValueError('only stationary, unlinked trampolines are supported')
    with args.initial.open(encoding='utf8', newline='') as stream:
        initial = [r for r in csv.DictReader(stream) if r['kind'] == 'init' and r['id'] == args.id]
    pending, step = ('1ce4ea8', '-1') if placement['IsContinueOpen'] else ('1ce4ea0', '0')
    if (len(initial) != 1 or initial[0]['current_n'] != '0'
            or initial[0]['pending_n'] != pending or initial[0]['step'] != step
            or initial[0]['connector'] != '0' or initial[0]['cooldown'] != '0'
            or initial[0]['close_counter'] != '300' or initial[0]['dead'] != '0'):
        raise ValueError('unsupported INITIAL source phase')
    archive = outfits.archive(args.romfs / 'ObjectData/CapTrampoline.szs')
    root['cap_trampolines'] = [dict(placement=placement,
                                   sensors=outfits.byml(archive['InitSensor.byml']),
                                   initial_phase='OpenPending' if placement['IsContinueOpen'] else 'WaitPending')]
    args.geometry.write_text(json.dumps(root, ensure_ascii=False, indent=2), encoding='utf8')


if __name__ == '__main__':
    main()
