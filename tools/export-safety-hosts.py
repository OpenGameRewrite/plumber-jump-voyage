"""Add verified static AskSafetyPoint replies from the user's StageData to a local export.

Only FixMapParts and RippleFixMapParts are covered. Unknown actor classes remain
unmarked: collision codes or an object's static flag do not establish its reply.
"""
import argparse
import importlib.util
import json
from pathlib import Path


def accepts(placement):
    actor = placement.get('UnitConfig', {}).get('ParameterConfigName')
    if actor == 'RippleFixMapParts':  # receiveMsg, 1.0.0 0x89ec44
        return True
    if actor == 'FixMapParts':  # receiveMsg, 1.0.0 0x913c38
        links = placement.get('Links', {})
        return not (links.get('SwitchAppear') or links.get('SwitchKill'))
    return False


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
    objects = {obj['Id']: obj for obj in placement.get('ObjectList', [])}
    count = 0
    for obj in geometry['objects']:
        source = objects.get(obj['id'])
        if source and source.get('UnitConfig', {}).get('ParameterConfigName') in ('FixMapParts', 'RippleFixMapParts'):
            obj['ask_safety_point'] = accepts(source)
            obj['safety_host_class'] = source['UnitConfig']['ParameterConfigName']
            count += int(obj['ask_safety_point'])
    args.geometry.write_text(json.dumps(geometry, separators=(',', ':')), encoding='utf8')
    print(f'{args.geometry}: {count} accepting static safety hosts')


if __name__ == '__main__':
    main()
