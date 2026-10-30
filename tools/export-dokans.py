"""Export a stationary linked Dokan pair from original placement/init resources.

No recorded poses, phases, player samples or message transcripts are inputs.
The native source computes the 2D mouth snap; raw collision matrices stay raw.
"""
import argparse
import importlib.util
import json
from pathlib import Path


def placements(value):
    if isinstance(value, dict):
        if 'Id' in value and 'UnitConfigName' in value:
            yield value
        for child in value.values():
            yield from placements(child)
    elif isinstance(value, list):
        for child in value:
            yield from placements(child)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('geometry', type=Path)
    parser.add_argument('--romfs', type=Path, required=True)
    parser.add_argument('--id', required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    if args.out.resolve() == args.geometry.resolve():
        raise ValueError('Use a separate output; preserve the original geometry')
    spec = importlib.util.spec_from_file_location('outfits', Path(__file__).with_name('convert-outfits.py'))
    outfits = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(outfits)
    root = json.loads(args.geometry.read_text(encoding='utf8'))
    stage_name = root['stage']
    archive = outfits.archive(args.romfs / 'StageData' / (stage_name + 'Map.szs'))
    stage = outfits.byml(archive[stage_name + 'Map.byml'])[root['scenario'] - 1]
    source = next(p for p in placements(stage) if p['Id'] == args.id)
    destinations = source.get('Links', {}).get('DstDokan', [])
    if source['UnitConfigName'] != 'Dokan' or len(destinations) != 1:
        raise ValueError('Expected one ordinary Dokan linked to Dokan2D')
    destination = destinations[0]
    if destination['UnitConfigName'] != 'Dokan2D':
        raise ValueError('Expected a Dokan2D destination')
    exported = []
    for placement, other in [(source, destination), (destination, source)]:
        if (placement.get('IsConnectToCollision') or placement.get('IsNoSnap')
                or placement.get('IsMoon') or placement.get('IsExitOnly')
                or placement.get('Scale') != dict(X=1., Y=1., Z=1.)):
            raise ValueError('Unsupported moving/no-snap/Moon/exit-only pipe')
        resource = 'Dokan2D' if placement['UnitConfigName'] == 'Dokan2D' else 'DokanChange2D'
        obj = next(o for o in root['objects'] if o['id'] == placement['Id'] and o['model'] == resource)
        init = outfits.archive(args.romfs / 'ObjectData' / (resource + '.szs'))
        exported.append(dict(placement=placement, destination_id=other['Id'],
                             raw_matrix=obj['matrix'], sensors=outfits.byml(init['InitSensor.byml'])))
    root['dokans'] = exported
    # Keep the original parameters for the independent camera-completion port.
    if 'CameraParam.byml' in archive:
        root['camera_parameters'] = outfits.byml(archive['CameraParam.byml'])
    output = json.dumps(root, ensure_ascii=False, indent=2) + '\n'
    if args.out.exists():
        if args.out.read_text(encoding='utf8') == output:
            print('Dokan export already matches original placement/init data')
            return
        raise FileExistsError('Output exists and differs; preserve it and choose another path')
    args.out.write_text(output, encoding='utf8')
    print('Exported linked Dokan pair:', source['Id'], destination['Id'])


if __name__ == '__main__':
    main()
