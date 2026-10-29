"""Append the stage's placed transparent coins (`CoinChameleon`) to local geometry.

The environment rule binds a probe-observed overlap with one of these actors to its exact
placement: the actor's origin is the placement's translation plus the UnitConfig display offset
(`tryAddDisplayOffset`), the same convention export-coin-look-sensors.py uses for `Coin`.
This supplies positions for evidence checks only, never a collision, damage or contact waiver.
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
    coins = []
    for obj in placement.get('ObjectList', []):
        if obj['UnitConfigName'] != 'CoinChameleon':
            continue
        offset = obj['UnitConfig'].get('DisplayTranslate', {})
        coins.append(dict(id=obj['Id'], layer=obj['LayerConfigName'],
                          translate=[obj['Translate'][a] for a in 'XYZ'],
                          display_offset=[offset.get(a, 0.) for a in 'XYZ'],
                          rotate=[obj['Rotate'][a] for a in 'XYZ'],
                          links=bool(obj.get('Links'))))
    geometry['coin_chameleons'] = coins
    args.geometry.write_text(json.dumps(geometry, separators=(',', ':')), encoding='utf-8')
    print(f"{args.geometry}: {len(coins)} CoinChameleon placements")


if __name__ == '__main__':
    main()
