"""Export OceanWave placement heights for the local deep-water query adapter.

OceanWaveActor::init installs Translate.Y as OceanWave::setHeight (0x2d2ee4);
preset5 never reports water. No water volume is inferred from a recording.
GPU wave/depth textures are not exported; animated surface parity is unverified.
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
    geometry['ocean_waves'] = [dict(id=o['Id'], height=o['Translate']['Y'], preset=o.get('ConfigPreset', 0))
                               for o in placement.get('NatureList', []) if o['UnitConfigName'] == 'OceanWave']
    args.geometry.write_text(json.dumps(geometry, separators=(',', ':')), encoding='utf-8')
    print(f"{args.geometry}: {len(geometry['ocean_waves'])} ocean placements")


if __name__ == '__main__':
    main()
