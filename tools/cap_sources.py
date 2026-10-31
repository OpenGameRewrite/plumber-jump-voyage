"""Placement-bound proof of Cappy's interactions with exported source actors.

The native replay initializes each source from its exported placement and InitSensor data
(`export-cap-sources.py`) and takes no actor side file. The passive probe files only validate it:
`game.cap_sources.csv` (actor lifecycle), `game.cap_messages.csv` (ordered messages and the cap's
scheduler observations), `game.cap.csv` and `game.cap_collider.csv`. A run is accepted only for the
exact stage, scenario, placements and message combinations below; the global Rebound/LockOn
environment flag stays in force for every other run.
"""
import struct
from collections import defaultdict

ID_NERVE = {'SignBoardNormal': '1d30278'}
SIGN_SENSORS = [{'MaxCount': 0, 'Name': 'Collision', 'Radius': 0., 'Type': 'CollisionParts'},
                {'Name': 'LookAt', 'Radius': 125., 'Type': 'LookAt', 'Y': 150.}]
# Perception queries every object with a Body/LookAt sensor answers for the player's and the cap's
# Eye; none of them changes the receiver (the look-at holder only records the candidate).
PERCEPTION = {'PlayerDisregard', 'PlayerDisregardHomingAttack', 'PlayerDisregardTargetMarker',
              'PlayerEyePriorityTarget', 'PlayerLookAtPosition', 'TargetMarkerPosition'}
# Hosts the cap's Eye sensor overlaps without a message: (class, sensor, sensor type).
EYE_HOSTS = {('ReactionObject', 'Body', '12'), ('CapHanger', 'Body', '12'), ('SignBoard', 'LookAt', '18')}
# Every accepted message of the run besides the source's own protocol: (message, sender sensor,
# receiver id, receiver sensor). The cap and Mario exchange these on every throw/catch.
CAP_PROTOCOL = {('PlayerDisregard', 'Eye', 'HackCap', 'Catch'), ('PlayerDisregard', 'Eye', 'HackCap', 'Trample'),
                ('PlayerCapCatch', 'Body', 'HackCap', 'Catch')}


def short(message):
    return message.split('::')[-1].replace('SensorMsg', '')


def position_bits(placement):
    """The f32 words of a placement's translation, as the probe writes them."""
    return ','.join(f"{struct.unpack('>I', struct.pack('>f', placement['Translate'][a]))[0]:08x}" for a in 'XYZ')


class Evidence:
    def __init__(self, geometry, cap, collider, sources, messages, rewrite, rows, preroll):
        self.frames, self.rebound, self.error = set(), set(), None
        self.signs, self.perceived = {}, set()
        try:
            self.verify(geometry, cap, collider, sources, messages, rewrite, rows, preroll)
        except (ValueError, KeyError, TypeError, IndexError, StopIteration, struct.error) as error:
            self.frames, self.rebound, self.signs = set(), set(), {}
            self.error = str(error)

    def verify(self, geometry, cap, collider, sources, messages, rewrite, rows, preroll):
        if not (geometry and cap and collider and sources and messages and rewrite and rows):
            return
        signs = geometry.get('signboards', [])
        if (geometry['stage'], geometry['scenario']) != ('SandWorldHomeStage', 1) or not signs:
            return
        ids = self.check_signs(signs)
        samples, initial = self.sample_rows(sources, ids)
        for identity in ids:
            if (initial[identity]['pending_n'], initial[identity]['current_n'], initial[identity]['step']) != (ID_NERVE['SignBoardNormal'], '0', '0'):
                raise ValueError(f'{identity} INITIAL phase')
        first, last = min(min(s) for s in samples.values()), max(max(s) for s in samples.values())
        own = {identity: (samples[identity][first]['actor'], samples[identity][first]['generation'])
               for identity in ids}
        for identity in ids:
            for f in range(first, last + 1):
                r = samples[identity][f]
                if ((r['actor'], r['generation']) != own[identity] or r['zone'] or r['registry_dropped'] != '0'
                        or r['dead'] != '0' or r['clipped'] != '0'
                        or r['pos_bits'] != position_bits(signs_by_id(signs)[identity]['placement'])):
                    raise ValueError(f'{identity} identity/state {f}')
        # Native lifecycle against the recorded one, frame by frame.
        for f, row in rewrite.items():
            if f < first or f > last:
                continue
            want = '|'.join(f"{identity}/{int(samples[identity][f]['current_n_name'] == 'Reaction')}/{samples[identity][f]['step']}"
                            for identity in ids)
            if row.get('cap_source') != want:
                raise ValueError(f'native source {f}')
        self.verify_messages(messages, ids, own)
        self.verify_rebound(messages, cap, collider, ids)
        self.frames = {f for f in rewrite if first <= f <= last}

    @staticmethod
    def check_signs(signs):
        ids = []
        for source in signs:
            p = source['placement']
            if (p['UnitConfigName'] != 'SignBoardNormal' or p['IsLinkDest'] or p['Links']
                    or p['Scale'] != dict(X=1., Y=1., Z=1.) or p['Rotate']['X'] != 0. or p['Rotate']['Z'] != 0.
                    or source['sensors'] != SIGN_SENSORS or source['collision'] != {'Sensor': 'Collision'}
                    or source['initial_phase'] != 'WaitPending'):
                raise ValueError('unsupported sign placement/sensors')
            ids.append(p['Id'])
        if ids != ['obj454']:
            raise ValueError('sign set')
        return sorted(ids)

    @staticmethod
    def sample_rows(rows, ids):
        samples, initial = {i: {} for i in ids}, {}
        for r in rows:
            if r['id'] not in ids:
                continue
            if r['kind'] == 'init':
                if r['id'] in initial:
                    raise ValueError('duplicate init')
                initial[r['id']] = r
            elif r['kind'] == 'sample':
                f = int(r['frame'])
                if f in samples[r['id']]:
                    raise ValueError('duplicate sample')
                samples[r['id']][f] = r
        if set(initial) != set(ids) or not all(samples.values()):
            raise ValueError('missing source rows')
        return samples, initial

    def verify_messages(self, messages, ids, own):
        """Only the ordered reflect, the rejected lock-on probes and perception queries occur."""
        accepted = []
        for r in messages:
            if r['kind'] != 'send_after':
                continue
            msg = short(r['message_type'])
            route = (msg, r['sender_sensor'], r['receiver_id'], r['receiver_sensor'])
            if r['receiver_id'] in ids:
                identity = (r['receiver_actor'], r['receiver_generation'])
                if msg == 'CapReflectCollide':
                    if r['answer'] == '1':
                        accepted.append(r)
                elif msg == 'CapStartLockOn':
                    if r['answer'] != '0' or route[1:] != ('Attack', r['receiver_id'], 'Collision'):
                        raise ValueError('lock-on answer')
                elif msg in PERCEPTION:
                    if r['answer'] != '0':
                        raise ValueError('source perception answer')
                else:
                    raise ValueError(f'unproved source message {msg}')
                if own[r['receiver_id']][1] != r['receiver_generation']:
                    raise ValueError(f'source generation {identity}')
            elif r['answer'] == '1' and msg not in PERCEPTION and route not in CAP_PROTOCOL:
                raise ValueError(f'unproved accepted message {msg}')
        if len(accepted) != 1:
            raise ValueError('accepted reflect count')
        r = accepted[0]
        if (r['sender_id'], r['sender_sensor'], r['receiver_id'], r['receiver_sensor'], r['cap_current_n_name']) != (
                'HackCap', 'Attack', 'obj454', 'Collision', 'Throw'):
            raise ValueError('reflect identity')
        self.reflect = int(r['frame'])

    def verify_rebound(self, messages, cap, collider, ids):
        """exeRebound: four consecutive executions, zero velocity after each, Return scheduled by the last."""
        runs = defaultdict(dict)
        for r in messages:
            if r['kind'] in ('rebound_before', 'rebound_after') and r['preroll'] == '0':
                runs[int(r['frame'])][r['kind']] = r
        frames = sorted(runs)
        if frames != [self.reflect + 1 + k for k in range(4)]:
            raise ValueError('rebound execution frames')
        zero = '00000000,00000000,00000000'
        for k, f in enumerate(frames):
            before, after = runs[f]['rebound_before'], runs[f]['rebound_after']
            if (int(before['cap_step']) != k or before['cap_current_n_name'] != 'Rebound'
                    or after['velocity_bits'] != zero or (k and before['velocity_bits'] != zero)):
                raise ValueError(f'rebound step {k}')
            if (k == 3) != (after['cap_pending_n_name'] == 'Return'):
                raise ValueError('rebound exit')
        # The collision the reflect asked: the wall hit of the preceding movement is the sign.
        wall = [r for r in collider if int(r['frame']) == self.reflect - 1 and r['preroll'] == '0']
        if len(wall) != 1 or (wall[0]['wall_hit_id'], wall[0]['wall_hit_sensor']) != ('obj454', 'Collision'):
            raise ValueError('reflect wall identity')
        states = {int(r['frame']): r['current_n_name'] for r in cap if r['kind'] == 'sample' and r['preroll'] == '0'}
        self.rebound = {f for f, name in states.items() if name == 'Rebound'}
        if self.rebound != {self.reflect, *frames[:3]}:
            raise ValueError('rebound sample frames')

    def reason(self, frame, text):
        return text.startswith('Cappy Rebound') and frame in self.rebound and frame in self.frames

    def contact(self, frame, c):
        return (frame in self.frames and c['who'] == 'cap' and c['kind'] == 'a' and not c['message']
                and not c['answer'] and (c['self'], c['self_type']) == ('Eye', '0')
                and (c['host_class'], c['other'], c['other_type']) in EYE_HOSTS)


def signs_by_id(signs):
    return {s['placement']['Id']: s for s in signs}


def fixture():
    """A minimal run: obj454 reacts to the reflect of frame 2 for 11 frames; the cap rebounds on 3..6."""
    placement = dict(Id='obj454', UnitConfigName='SignBoardNormal', IsLinkDest=False, Links={},
                     Translate=dict(X=-4800., Y=100., Z=-19150.), Rotate=dict(X=0., Y=90., Z=0.),
                     Scale=dict(X=1., Y=1., Z=1.), UnitConfig=dict(DisplayName='sign'))
    geometry = dict(stage='SandWorldHomeStage', scenario=1, signboards=[dict(
        placement=placement, sensors=SIGN_SENSORS, collision={'Sensor': 'Collision'}, initial_phase='WaitPending')])
    bits = position_bits(placement)
    base = dict(zone='', actor='6', generation='6', registry_dropped='0', dead='0', clipped='0', pos_bits=bits)
    sources = [dict(base, frame='-2147483648', preroll='0', kind='init', id='obj454', current_n='0',
                    pending_n=ID_NERVE['SignBoardNormal'], step='0', current_n_name='')]
    rewrite, rows, native = {}, {}, {}
    for f in range(0, 16):
        reacting = 2 <= f <= 12
        step = f - 1 if reacting else f if f < 2 else f - 13
        sources.append(dict(base, frame=str(f), preroll='0', kind='sample', id='obj454', current_n='1',
                            pending_n='0', step=str(step), current_n_name='Reaction' if reacting else 'Wait'))
        rewrite[f] = dict(cap_source=f"obj454/{int(reacting)}/{step}")
        rows[f] = {}
    msg = dict(preroll='0', sender_id='HackCap', sender_sensor='Attack', receiver_id='obj454',
               receiver_sensor='Collision', receiver_actor='6', receiver_generation='6',
               message_type='rs::SensorMsgCapReflectCollide', cap_current_n_name='Throw')
    messages = [dict(msg, frame='2', kind='send_after', answer='1')]
    zero = '00000000,00000000,00000000'
    for k in range(4):
        messages.append(dict(preroll='0', frame=str(3 + k), kind='rebound_before', cap_step=str(k),
                             cap_current_n_name='Rebound', velocity_bits='c2000000,0,0' if k == 0 else zero))
        messages.append(dict(preroll='0', frame=str(3 + k), kind='rebound_after', cap_step=str(k),
                             cap_current_n_name='Rebound', velocity_bits=zero,
                             cap_pending_n_name='Return' if k == 3 else ''))
    collider = [dict(preroll='0', frame='1', wall_hit_id='obj454', wall_hit_sensor='Collision')]
    cap = [dict(preroll='0', kind='sample', frame=str(f), current_n_name='Rebound' if f in (2, 3, 4, 5) else 'Throw')
           for f in range(0, 7)]
    return geometry, cap, collider, sources, messages, rewrite, rows, {}


def self_check():
    from copy import deepcopy
    assert not Evidence(None, None, None, None, None, {}, {}, {}).frames
    assert short('rs::SensorMsgCapReflectCollide') == 'CapReflectCollide'
    args = fixture()
    evidence = Evidence(*args)
    assert evidence.frames == set(range(16)) and evidence.rebound == {2, 3, 4, 5}, evidence.error
    for change in ('stage', 'answer', 'sender', 'wall', 'rebound', 'message', 'placement'):
        bad = deepcopy(args)
        if change == 'stage':
            bad[0]['scenario'] = 2
        elif change == 'answer':
            bad[4][0]['answer'] = '0'
        elif change == 'sender':
            bad[4][0]['sender_sensor'] = 'Eye'
        elif change == 'wall':
            bad[2][0]['wall_hit_id'] = 'obj455'
        elif change == 'rebound':
            bad[4][5]['velocity_bits'] = '40000000,0,0'
        elif change == 'message':
            bad[4].append(dict(bad[4][0], frame='9', message_type='rs::SensorMsgPlayerAttackSpinAttack'))
        else:
            bad[0]['signboards'][0]['placement']['Translate']['X'] = -4799.
        assert not Evidence(*bad).frames, change
    contact = dict(who='cap', kind='a', message='', answer='', self='Eye', self_type='0',
                   host_class='SignBoard', other='LookAt', other_type='18')
    ok = Evidence(None, None, None, None, None, {}, {}, {})
    ok.frames = {4}
    assert ok.contact(4, contact) and not ok.contact(5, contact)
    assert not ok.contact(4, dict(contact, other='Body')) and not ok.contact(4, dict(contact, who='player'))


if __name__ == '__main__':
    self_check()
