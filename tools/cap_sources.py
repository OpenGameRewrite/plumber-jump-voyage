"""Placement-bound proof of Cappy's interactions with exported source actors.

The native replay initializes each source from its exported placement and InitSensor data
(`export-cap-sources.py`) and takes no actor side file. The passive probe files only validate it:
`game.cap_sources.csv` (actor lifecycle), `game.cap_messages.csv` (ordered messages and the cap's
scheduler observations), `game.cap.csv` and `game.cap_collider.csv`. A run is accepted only for the
exact stage, scenario, placements and message combinations below; the global Rebound/LockOn
environment flag stays in force for every other run.
"""
import re
import struct
from collections import defaultdict

ID_NERVE = {'SignBoardNormal': '1d30278', 'CapHanger': '1ce0b60', 'CapSwitchSave': '1ce3e98'}
HANGER_SENSORS = [{'Name': 'Body', 'Radius': 60., 'Type': 'MapObj', 'Y': 25.}]
SIGN_SENSORS = [{'MaxCount': 0, 'Name': 'Collision', 'Radius': 0., 'Type': 'CollisionParts'},
                {'Name': 'LookAt', 'Radius': 125., 'Type': 'LookAt', 'Y': 150.}]
# Perception queries every object with a Body/LookAt sensor answers for the player's and the cap's
# Eye; none of them changes the receiver (the look-at holder only records the candidate).
PERCEPTION = {'PlayerDisregard', 'PlayerDisregardHomingAttack', 'PlayerDisregardTargetMarker',
              'PlayerEyePriorityTarget', 'PlayerLookAtPosition', 'TargetMarkerPosition'}
# Hosts the cap's Eye sensor overlaps without a message: (class, sensor, sensor type).
EYE_HOSTS = {('ReactionObject', 'Body', '12'), ('CapHanger', 'Body', '12'), ('CapSwitch', 'Body', '12'),
             ('SignBoard', 'LookAt', '18')}
# Every accepted message of the run besides the source's own protocol: (message, sender sensor,
# receiver id, receiver sensor). The cap and Mario exchange these on every throw/catch.
CAP_PROTOCOL = {('PlayerDisregard', 'Eye', 'HackCap', 'Catch'), ('PlayerDisregard', 'Eye', 'HackCap', 'Trample'),
                ('PlayerCapCatch', 'Body', 'HackCap', 'Catch')}
# The hanger's answers (`CapHanger::receiveMsg` 0x235684) to what the cap's Attack sensor sends it,
# by message: the accepted answers.
HANGER_ANSWERS = {'CapAttackStayRolling': '0', 'CapCancelLockOn': '1', 'CapIgnoreCancelLockOn': '0',
                  'CapIgnoreCollisionCheck': '1', 'CapItemGet': '0', 'CapKeepLockOn': '01', 'CapReflect': '0',
                  'CapStartLockOn': '1', 'InitCapTarget': '1', 'StartHack': '0'}
# The switch's (`CapSwitch::receiveMsg`): it does not answer `CapIgnoreCollisionCheck`, and keeps the cap
# only while it holds it.
SWITCH_ANSWERS = dict(HANGER_ANSWERS, CapIgnoreCollisionCheck='0')
# The switch's lock-on matrix: the HatTarget joint of its own model; its activation reaction.
SWITCH_PLACEMENT = dict(Translate=dict(X=-1550., Y=-900., Z=-13400.), Rotate=dict(X=0., Y=0., Z=0.),
                        Scale=dict(X=1., Y=1., Z=1.))
SWITCH_SENSORS = [{'Name': 'Body', 'Radius': 100., 'Type': 'MapObj', 'Y': 64.},
                  {'MaxCount': 0, 'Name': 'Collision', 'Radius': 0., 'Type': 'CollisionParts'}]


def short(message):
    return message.split('::')[-1].replace('SensorMsg', '')


def position_bits(placement):
    """The f32 words of a placement's translation, as the probe writes them."""
    return ','.join(f"{struct.unpack('>I', struct.pack('>f', placement['Translate'][a]))[0]:08x}" for a in 'XYZ')


class Evidence:
    def __init__(self, geometry, cap, collider, sources, messages, rewrite, rows, preroll):
        self.frames, self.rebound, self.lockon, self.attack, self.error = set(), set(), set(), set(), None
        self.reflect = self.keeps = self.start = self.probes = None
        self.lock_class = self.lock_id = None
        try:
            self.verify(geometry, cap, collider, sources, messages, rewrite, rows, preroll)
        except (ValueError, KeyError, TypeError, IndexError, StopIteration, struct.error) as error:
            self.frames, self.rebound, self.lockon, self.attack = set(), set(), set(), set()
            self.error = str(error)

    def verify(self, geometry, cap, collider, sources, messages, rewrite, rows, preroll):
        if not (geometry and cap and collider and sources and messages and rewrite and rows):
            return
        signs, hangers = geometry.get('signboards', []), geometry.get('cap_hangers', [])
        switches = geometry.get('cap_switches', [])
        if (geometry['stage'], geometry['scenario']) != ('SandWorldHomeStage', 1) or not (signs or switches):
            return
        sign_ids, hanger_ids = self.check_signs(signs), self.check_hangers(hangers)
        switch_ids = self.check_switches(switches)
        lock_ids = hanger_ids + switch_ids
        ids = sign_ids + hanger_ids + switch_ids
        placements = {s['placement']['Id']: s['placement'] for s in signs + hangers + switches}
        samples, initial = self.sample_rows(sources, ids)
        for identity in ids:
            nerve = ID_NERVE['CapHanger' if identity in hanger_ids else
                             'CapSwitchSave' if identity in switch_ids else 'SignBoardNormal']
            if (initial[identity]['pending_n'], initial[identity]['current_n'], initial[identity]['step']) != (nerve, '0', '0'):
                raise ValueError(f'{identity} INITIAL phase')
            if identity in hanger_ids and (initial[identity]['quota'], initial[identity]['generated'],
                                          initial[identity]['item_type']) != ('3', '0', '0'):
                raise ValueError(f'{identity} INITIAL items')
        first, last = min(min(s) for s in samples.values()), max(max(s) for s in samples.values())
        own = {identity: (samples[identity][first]['actor'], samples[identity][first]['generation'])
               for identity in ids}
        for identity in ids:
            for f in range(first, last + 1):
                r = samples[identity][f]
                if ((r['actor'], r['generation']) != own[identity] or r['zone'] or r['registry_dropped'] != '0'
                        or r['dead'] != '0' or r['clipped'] != '0'
                        or r['pos_bits'] != position_bits(placements[identity])
                        or (identity in hanger_ids and (r['quota'], r['item_type']) != ('3', '0'))):
                    raise ValueError(f'{identity} identity/state {f}')

        # Native lifecycle against the recorded one, frame by frame.
        def record(identity, f):
            r = samples[identity][f]
            if identity in hanger_ids:
                return f"{identity}/{r['current_n_name']}/{r['step']}/{r['generated']}"
            if identity in switch_ids:
                # `exeOnWait` does nothing after its first step, and the step counts the world's ticks:
                # the linked door's camera demo runs the switch while Mario's own clock stands still.
                step = '*' if r['current_n_name'] == 'OnWait' else r['step']
                return f"{identity}/{r['current_n_name']}/{step}"
            return f"{identity}/{int(r['current_n_name'] == 'Reaction')}/{r['step']}"

        def native(text):
            return re.sub(r'(obj\d+/OnWait)/\d+', r'\1/*', text) if switch_ids else text
        for f, row in rewrite.items():
            if first <= f <= last and native(row.get('cap_source', '')) != '|'.join(record(i, f) for i in ids):
                raise ValueError(f'native source {f}')
        self.verify_messages(messages, sign_ids, hanger_ids, switch_ids, own)
        if lock_ids:
            self.lock_id = lock_ids[0]
            self.lock_class = 'CapHanger' if hanger_ids else 'CapSwitchSave'
            self.verify_lock_on(messages, cap, lock_ids[0], bool(hanger_ids))
        else:
            self.verify_rebound(messages, cap, collider)
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
        if ids not in ([], ['obj454']):
            raise ValueError('sign set')
        return ids

    @staticmethod
    def check_hangers(hangers):
        ids = []
        for source in hangers:
            p = source['placement']
            if (p['UnitConfigName'] != 'CapHanger' or p['IsLinkDest'] or p['IsConnectToCollision'] or p['Links']
                    or p['ItemType'] != 'Coin' or p['AppearItemNum'] != 3 or p['SensorRadius'] != 60.
                    or p['Scale'] != dict(X=1., Y=1., Z=1.) or p['Rotate'] != dict(X=0., Y=90., Z=0.)
                    or p['Translate'] != dict(X=-4800., Y=327.65618896484375, Z=-19150.)
                    or source['sensors'] != HANGER_SENSORS or source['initial_phase'] != 'WaitPending'):
                raise ValueError('unsupported hanger placement/sensors')
            ids.append(p['Id'])
        if ids not in ([], ['obj4377']):
            raise ValueError('hanger set')
        return ids

    @staticmethod
    def check_switches(switches):
        ids = []
        for source in switches:
            p = source['placement']
            if (p['UnitConfigName'] != 'CapSwitchSave' or p['IsLinkDest'] or p['IsConnectToCollision']
                    or p['IsNoReaction'] or list(p['Links']) != ['RiseParts'] or p['Id'] != 'obj2025'
                    or any(p[k] != v for k, v in SWITCH_PLACEMENT.items())
                    or source['sensors'] != SWITCH_SENSORS or source['initial_phase'] != 'WaitPending'
                    or source['hack_cap']['JointName'] != 'HatTarget' or source['hack_cap']['LockOnOnly'] is not True):
                raise ValueError('unsupported switch placement/sensors')
            ids.append(p['Id'])
        return ids

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

    def verify_messages(self, messages, sign_ids, hanger_ids, switch_ids, own):
        """Only the ordered reflect/lock-on protocol, rejected probes and perception queries occur."""
        accepted, start, keeps, probes = [], [], [], []
        for r in messages:
            if r['kind'] != 'send_after':
                continue
            msg = short(r['message_type'])
            route = (msg, r['sender_sensor'], r['receiver_id'], r['receiver_sensor'])
            if r['receiver_id'] in own and own[r['receiver_id']][1] != r['receiver_generation']:
                raise ValueError(f"source generation {r['receiver_id']}")
            if r['receiver_id'] in sign_ids:
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
            elif r['receiver_id'] in hanger_ids or r['receiver_id'] in switch_ids:
                sender = (r['sender_id'], r['sender_sensor'])
                answers = HANGER_ANSWERS if r['receiver_id'] in hanger_ids else SWITCH_ANSWERS
                if msg in answers:
                    if r['answer'] not in answers[msg] or (msg != 'StartHack' and sender != ('HackCap', 'Attack')):
                        raise ValueError(f'lock-on target answer {msg}')
                    if msg == 'CapStartLockOn':
                        start.append(r)
                    elif msg == 'CapKeepLockOn':
                        keeps.append(r)
                    elif msg in ('CapIgnoreCancelLockOn', 'CapCancelLockOn'):
                        probes.append(r)
                elif msg in PERCEPTION:
                    if msg != 'TargetMarkerPosition' and r['answer'] != '0':
                        raise ValueError('hanger perception')
                else:
                    raise ValueError(f'unproved hanger message {msg}')
            elif r['answer'] == '1' and msg not in PERCEPTION and route not in CAP_PROTOCOL:
                raise ValueError(f'unproved accepted message {msg}')
        if hanger_ids or switch_ids:
            self.keeps, self.start, self.probes = keeps, start, probes
            if accepted or len(start) != 1:
                raise ValueError('lock-on start count')
            return
        if len(accepted) != 1:
            raise ValueError('accepted reflect count')
        r = accepted[0]
        if (r['sender_id'], r['sender_sensor'], r['receiver_id'], r['receiver_sensor'], r['cap_current_n_name']) != (
                'HackCap', 'Attack', 'obj454', 'Collision', 'Throw'):
            raise ValueError('reflect identity')
        self.reflect = int(r['frame'])

    def verify_lock_on(self, messages, cap, identity, coins):
        """CapStartLockOn/InitCapTarget accepted once, kept every execution, refused once, then tryReturn."""
        start = self.start[0]
        if (start['sender_id'], start['sender_sensor'], start['receiver_sensor'], start['cap_current_n_name'],
                start['answer']) != ('HackCap', 'Attack', 'Body', 'Throw', '1'):
            raise ValueError('start identity')
        first = int(start['frame'])
        targets = [r for r in messages if r['kind'] == 'send_after' and short(r['message_type']) == 'InitCapTarget'
                   and r['answer'] == '1' and r['receiver_id'] == identity]
        if len(targets) != 1 or int(targets[0]['frame']) != first:
            raise ValueError('InitCapTarget')
        kept = sorted(int(r['frame']) for r in self.keeps if r['answer'] == '1')
        refused = [r for r in self.keeps if r['answer'] == '0']
        if len(refused) != 1 or kept != list(range(first, int(refused[0]['frame']))):
            raise ValueError('keep sequence')
        end = int(refused[0]['frame'])
        # The refusal is followed by tryReturn: ignore-cancel refused, cancel accepted, Return pending.
        answers = sorted((short(r['message_type']), r['answer'], int(r['frame'])) for r in self.probes)
        if answers != [('CapCancelLockOn', '1', end), ('CapIgnoreCancelLockOn', '0', end)]:
            raise ValueError('tryReturn messages')
        tries = [r for r in messages if r['kind'] == 'try_return_after' and r['preroll'] == '0']
        if len(tries) != 1 or (int(tries[0]['frame']), tries[0]['answer'], tries[0]['cap_pending_n_name']) != (end, '1', 'Return'):
            raise ValueError('tryReturn result')
        # The multi-coin helper succeeds on the first execution of each tenth step.
        generated = sorted(int(r['frame']) for r in messages if r['kind'] == 'multi_coin_after'
                           and r['answer'] == '1' and r['preroll'] == '0')
        if generated != ([first, first + 10, first + 20] if coins else []):
            raise ValueError('coin cadence')
        samples = {int(r['frame']): r for r in cap if r['kind'] == 'sample' and r['preroll'] == '0'}
        for f in range(first, end):
            r = samples[f]
            if (r['current_n_name'] != 'LockOn' or r['target_id'] != identity
                    or int(r['counter584']) != min(f - first + 1, 4) or int(r['counter580']) != min(f - first + 1, 10)):
                raise ValueError(f'lock-on counters {f}')
        self.lockon = set(range(first, end))
        self.attack = {int(r['frame']) for r in messages if r['kind'] == 'send_after'
                       and short(r['message_type']) == 'CapItemGet' and r['receiver_id'] == identity}

    def verify_rebound(self, messages, cap, collider):
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
        return self.accepted(frame, text) is not None

    def accepted(self, frame, text):
        """(label, why) of a Rebound/LockOn flag the verified run explains, else None."""
        if frame not in self.frames:
            return None
        if text.startswith('Cappy Rebound') and frame in self.rebound:
            return ('exported SignBoardNormal Rebound',
                    'accepted CapReflectCollide of obj454, ordered four-step zero-velocity exeRebound, '
                    'verified collider identity')
        if text.startswith('Cappy LockOn') and frame in self.lockon:
            return (f'exported {self.lock_class} LockOn',
                    f'{self.lock_id}: CapStartLockOn/InitCapTarget accepted once, CapKeepLockOn every execution, '
                    'then tryReturn; the rewrite simulated the source from its exported placement and sensors')
        return None

    def contact(self, frame, c):
        if frame not in self.frames or c['who'] != 'cap' or c['kind'] != 'a' or c['message'] or c['answer']:
            return False
        host = (c['host_class'], c['other'], c['other_type'])
        if (c['self'], c['self_type']) == ('Eye', '0'):
            return host in EYE_HOSTS
        # The cap's Attack overlap with the hanger is the one that sends its protocol on this frame.
        return ((c['self'], c['self_type']) == ('Attack', '4')
                and host in {('CapHanger', 'Body', '12'), ('CapSwitch', 'Body', '12')} and frame in self.attack)


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
    rewrite, rows = {}, {}
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
    for change in ('stage', 'answer', 'sender', 'wall', 'rebound', 'message', 'placement', 'hanger', 'switch'):
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
        elif change == 'hanger':
            # A hanger placement that is not the recorded obj4377 is refused, whatever the rest says.
            bad[0]['cap_hangers'] = [dict(placement=dict(UnitConfigName='CapHanger', Id='obj1', IsLinkDest=False,
                                                         IsConnectToCollision=False, Links={}, ItemType='Coin',
                                                         AppearItemNum=3, SensorRadius=60.,
                                                         Scale=dict(X=1., Y=1., Z=1.), Rotate=dict(X=0., Y=90., Z=0.),
                                                         Translate=dict(X=0., Y=0., Z=0.)),
                                          sensors=HANGER_SENSORS, initial_phase='WaitPending')]
        elif change == 'switch':
            # A switch placement other than the recorded obj2025 is refused with an otherwise
            # complete record (the sign alone would be accepted).
            bad[0]['cap_switches'] = [dict(placement=dict(UnitConfigName='CapSwitchSave', Id='obj2026',
                                                          IsLinkDest=False, IsConnectToCollision=False,
                                                          IsNoReaction=False, Links={'RiseParts': []},
                                                          **SWITCH_PLACEMENT),
                                           sensors=SWITCH_SENSORS, initial_phase='WaitPending',
                                           hack_cap=dict(JointName='HatTarget', LockOnOnly=True))]
        else:
            bad[0]['signboards'][0]['placement']['Translate']['X'] = -4799.
        assert not Evidence(*bad).frames, change
    contact = dict(who='cap', kind='a', message='', answer='', self='Eye', self_type='0',
                   host_class='SignBoard', other='LookAt', other_type='18')
    ok = Evidence(None, None, None, None, None, {}, {}, {})
    ok.frames, ok.attack, ok.lockon = {4}, {4}, {4}
    assert ok.contact(4, contact) and not ok.contact(5, contact)
    assert not ok.contact(4, dict(contact, other='Body')) and not ok.contact(4, dict(contact, who='player'))
    attack = dict(contact, self='Attack', self_type='4', host_class='CapHanger', other='Body', other_type='12')
    assert ok.contact(4, attack) and not ok.contact(5, attack)
    ok.attack = set()
    assert not ok.contact(4, attack)
    assert ok.reason(4, 'Cappy LockOn (hit/captured something)') and not ok.reason(5, 'Cappy LockOn')
    assert not ok.reason(4, 'Cappy Hack')


if __name__ == '__main__':
    self_check()
