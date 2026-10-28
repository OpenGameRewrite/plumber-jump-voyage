"""Independent verification of exported SandGeyser sources and their sampled phase.

All actor evidence is separate from Mario's trace. Scalar operations follow 1.0.0.
"""
import struct


def f32(x):
    return struct.unpack('<f', struct.pack('<f', x))[0]


def bits(x):
    return struct.pack('>f', x).hex()


PHASES = ['Wait', 'UpStart', 'UpLoop', 'UpWait', 'UpEnd', 'UpEndWait']


class Source:
    def __init__(self, placement):
        self.placement = placement
        self.phase, self.step, self.top = 0, 0, 0.
        self.radius, self.valid = 450., True
        self.centre = list(placement['position'])

    def tick(self):
        p = self.placement
        phase, step = self.phase, self.step
        next_phase = None
        if phase == 0 and step == p['up_start']:
            next_phase = 1
        elif phase == 1:
            if step == 0:
                self.top = 0.
            if step == 180:
                next_phase = 2
        elif phase == 2:
            self.valid = True
            self.top = f32(f32(p['target_y'] - p['position'][1]) * f32(step / p['up_time']))
            if step <= p['up_time']:
                self.radius = max(f32(f32(self.top * .5) + 200.), 175.)
                self.centre[1] = f32(f32(p['position'][1] - 200.) + self.radius)
            else:
                next_phase = 3
        elif phase == 3 and step == 300:
            next_phase = 4
        elif phase == 4:
            self.valid = False
            if step == 30:
                next_phase = 5
        elif phase == 5 and step == p['end_wait']:
            next_phase = 1
        if next_phase is None:
            self.step += 1
        else:
            self.phase, self.step = next_phase, 0

    def diagnostic(self):
        return f"{self.placement['id']}/{self.phase}/{self.step}/{bits(self.top)}/{bits(self.radius)}/{int(self.valid)}"


class Evidence:
    def __init__(self, geometry, actor_rows, rewrite, seed):
        self.sources, self.tokens = {}, {}
        self.frames = {}
        if not geometry or not actor_rows or not seed:
            return
        placements = geometry.get('sand_geysers', [])
        if not placements or (seed.get('stage'), seed.get('scenario')) != (geometry['stage'], geometry['scenario']):
            return
        sources = {p['id']: Source(p) for p in placements}
        by_frame = {}
        for row in actor_rows:
            by_frame.setdefault(int(row['frame']), []).append(row)
        first = min(by_frame)
        if seed.get('frame') != first or first >= 0:
            return
        tokens, frames = {}, {}
        for frame in range(first, max(by_frame) + 1):
            rows = by_frame.get(frame, [])
            if {r['id'] for r in rows} != set(sources) or len(rows) != len(sources):
                return
            for row in rows:
                source = sources[row['id']]
                p = source.placement
                part = next((o for o in geometry['objects'] if o['id'] == p['id']), {})
                if (part.get('model') != 'SandGeyser' or part.get('translate') != p['position']
                        or part.get('rotate') != [0, 0, 0] or part.get('scale') != [1, 1, 1]
                        or not geometry.get('models', {}).get('SandGeyser#0', {}).get('prisms')
                        or any(row[k] != '0' for k in ('dead', 'clipped', 'registry_dropped'))
                        or [float(row['init_' + a]) for a in 'xyz'] != p['position']
                        or row['phase'] != PHASES[source.phase] or int(row['step']) != source.step
                        or row['top_local_bits'] != bits(source.top)
                        or row['radius_bits'] != bits(source.radius)
                        or [bits(float(row['centre_' + a])) for a in 'xyz'] != [bits(v) for v in source.centre]
                        or row['valid'] != str(int(source.valid)) or row['system'] != '1'):
                    return
                if row['actor'] != '0':
                    if row['actor'] in tokens and tokens[row['actor']] != p['id']:
                        return
                    tokens[row['actor']] = p['id']
                frames[frame, p['id']] = (source.top, source.radius, list(source.centre), source.valid)
            if frame >= 0:
                diagnostic = '|'.join(source.diagnostic() for source in sources.values())
                collision = rewrite.get(frame, {}).get('collision', '')
                if f'geysers={diagnostic}' not in collision:
                    return
            for source in sources.values():
                source.tick()
        self.sources, self.tokens, self.frames = {p['id']: p for p in placements}, tokens, frames

    def floor(self, frame, host):
        if not host or host.get('base') is None:
            return False
        for identity, p in self.sources.items():
            state = self.frames.get((frame, identity))
            if state and state[3] and host['host'] == p['host'] and host['sensor'] == 'Body':
                base = list(p['position'])
                base[1] = f32(base[1] + state[0])
                # Probe prints six decimals; compare in its own decimal representation.
                if tuple(round(v, 6) for v in base) == host['base']:
                    return True
        return False

    def contact(self, frame, contact):
        identity = self.tokens.get(contact['actor'])
        p = self.sources.get(identity)
        if (not p or contact['who'] != 'player' or contact['host_class'] != 'SandGeyser'
                or contact['host'] != p['host'] or contact['other'] != 'Raise' or contact['other_type'] != '12'):
            return False
        if contact['kind'] == 'a':
            return not contact['message'] and not contact['answer']
        return (contact['kind'] in ('r', 'p') and contact['message'] == 'rs::SensorMsgSandGeyserRaise'
                and contact['self_type'] in ('1', '3') and contact['answer'] in ('0', '1')
                and (contact['kind'] != 'p' or contact['answer'] == '0'))
