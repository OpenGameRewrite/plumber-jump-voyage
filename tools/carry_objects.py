"""Identity-bound flower contact evidence. Observations validate causes, never drive replay."""
import math
import struct
from collections import defaultdict


def f32(x):
    return struct.unpack('<f', struct.pack('<f', x))[0]


def words(values):
    return '/'.join(f'{struct.unpack("<I", struct.pack("<f", x))[0]:08x}' for x in values)


def values(row, key):
    return [float(x) for x in row[key + '_values'].split('/')]


def exact_vector(a, b):
    def decode(text):
        return [struct.unpack("<f", bytes.fromhex(x)[::-1])[0] for x in text.replace(",", "/").split("/")]
    return decode(a) == decode(b)  # exact f32 values; signed zero has the same value


def diagnostic(row):
    return dict(part.split('=', 1) for part in row.get('carry_source', '').split(';') if '=' in part)


def hold_matrix(attachment, throw_hand=False):
    """calcPlayerHoldMtx 0x5727e0, using independently observed joints for validation only."""
    spine = values(attachment, 'Spine2')
    a = f32(f32(2. ** -0.5) * f32(2. ** 0.5))
    d = f32(1. - a)
    z, y = [[d,a,0.],[-a,d,0.],[0.,0.,1.]], [[d,0.,-a],[0.,1.,0.],[a,0.,d]]
    rows = [spine[i:i+3] for i in (0,4,8)]
    for rotation in (z,y):
        rows = [[f32(math.fma(row[2], rotation[2][c],
                     f32(math.fma(row[1], rotation[1][c], f32(row[0]*rotation[0][c])))))
                 for c in range(3)] for row in rows]
    left, right = values(attachment,'HandL'), values(attachment,'HandR')
    position = ([f32(right[4*i+3] + f32(right[4*i] * 30.)) for i in range(3)] if throw_hand else
                [f32(f32(left[4*i+3]+right[4*i+3])*0.5) for i in range(3)])
    return [v for i,row in enumerate(rows) for v in row + [position[i]]]


def mario_answer(message, sensor):
    """receiveMsg 0x428810: attack-sensor veto, then ordered typed handlers, false by default.

    Handlers outside this flower composition return None (unproved), never an exemption.
    HitGrowFlowerPot has no entry here: its rejection follows the dispatch default.
    """
    if sensor in ('Carry', 'HipDropKnockDown', 'SpinAttack'):
        return False
    kind = message.split('SensorMsg')[-1]
    if kind == 'PushToPlayer':
        return True
    for family in (('BindEnd',), ('TransferHack', 'InitHack', 'EndHack'),
                   ('ObjSnapForce', 'ByugoBlow', 'AirExplosion'), ('CollidePush',),
                   ('Hit', 'HitStrong', 'HitVeryStrong', 'PlayerDamageBlowDown', 'CapObjHipDropReflect'),
                   ('PlayerJump', 'PlayerJumpRequest', 'PlayerJumpRequestAll', 'PlayerSpinJump', 'PlayerTrampleJump'),
                   ('PlayerDamage',), ('HoldCancel',), ('SandGeyserRaise',),
                   ('RequestPlayerWaitAnim',), ('RequestPlayerStainWet',)):
        if kind in family:
            return None
    return False


class Evidence:
    def __init__(self, geometry, objects, mario, messages, rewrite, rows, preroll):
        self.frames, self.identity, self.pushing = set(), None, set()
        self.passive = set()
        # The observed interval only defines the obligation to prove the simulation. It
        # supplies no movement, release timing or object state to the native replay.
        throws = [int(r['frame']) for r in (mario or []) if r.get('kind') == 'sample'
                  and (r.get('keeper_pending_n_name') or r.get('keeper_current_n_name')) == 'Throw'
                  and r.get('held_id') == 'obj2876']
        self.required = {f for f in rows if throws and f >= min(throws)} if rows else set()
        self.error = None
        try:
            self.verify(geometry, objects, mario, messages, rewrite, rows, preroll)
        except (ValueError, KeyError, TypeError, IndexError, StopIteration, OverflowError, struct.error) as error:
            self.frames, self.identity, self.pushing = set(), None, set()
            self.error = str(error)

    def verify(self, geometry, objects, mario, messages, rewrite, rows, preroll):
        if not geometry or not objects or not mario or not messages or not rewrite:
            return
        sources = geometry.get('carry_objects', [])
        if (geometry.get('stage'), geometry.get('scenario')) != ('SandWorldHomeStage', 1) or not sources:
            return
        source = next(s for s in sources if s['id'] in ('obj2873', 'obj2876'))
        if len(sources) > 1:
            if {s['id'] for s in sources} != {'obj2873','obj2872','obj2871'}:
                return
            for neighbour in sources:
                if neighbour['id'] == source['id']:
                    continue
                place = neighbour['placement']
                observed = [o for o in objects if o['id'] == neighbour['id'] and o['kind'] == 'sample']
                init = next(o for o in objects if o['id'] == neighbour['id'] and o['kind'] == 'init' and o['phase'] == 'init')
                if (place['UnitConfigName'] != 'GrowFlowerPot' or neighbour['parent'] != 'obj6326'
                        or neighbour['passive_sensors'] != [dict(Name='SeedNear',Radius=600.,Y=50.)]
                        or values(init,'pos') != [place['Translate'][a] for a in 'XYZ']
                        or any(o['pos_bits'] != init['pos_bits'] or o['quat_bits'] != init['quat_bits']
                               or o['dead'] != '0' for o in observed)
                        or any(e['kind'] == 'mario_receive_after' and e['sender_id'] == neighbour['id'] for e in messages)):
                    return
                self.passive.update((o['actor'], 'GrowFlowerPot', neighbour['host']) for o in observed)
        p = source['placement']
        cls = p['UnitConfigName']
        if (source['parent'] != 'obj6326' or source['id'] != p['Id']
                or source['initial_phase'] != 'init' or source['initial_alive'] is not True
                or p['Scale'] != dict(X=1., Y=1., Z=1.)
                or (cls, p['Id']) not in (('GrowFlowerPot', 'obj2873'), ('GrowFlowerSeed', 'obj2876'))):
            return
        initial = [o for o in objects if o['kind'] == 'init' and o['phase'] == 'init' and o['id'] == source['id']]
        if len(initial) != 1 or initial[0]['dead'] != '0':
            return
        position = [p['Translate'][a] for a in 'XYZ']
        if values(initial[0], 'pos') != position:
            return
        samples = {int(o['frame']): o for o in objects if o['kind'] == 'sample' and o['id'] == source['id']}
        keepers = {int(o['frame']): o for o in mario if o['kind'] == 'sample'}
        pushed = {int(o['frame']): o for o in mario if o['kind'] == 'pushed_velocity'}
        attachments = {int(o['frame']): o for o in mario if o['kind'] == 'attachment'}
        events = defaultdict(list)
        for e in messages:
            events[int(e['frame'])].append(e)
        frames, pushing, identity = set(), set(), None
        for frame, row in rows.items():
            obj, keeper, velocity = samples[frame], keepers[frame], pushed[frame]
            token = obj['actor']
            if not token or token == '0' or keeper['dropped'] != '0' or obj['registry_dropped'] != '0':
                raise ValueError('missing object identity or dropped observations')
            current_identity = (token, cls, source['host'])
            if identity is not None and identity != current_identity:
                raise ValueError('object identity changed')
            identity = current_identity
            if obj['dead'] != '0' or obj['clipped'] != '0' or obj['parent'] != source['parent']:
                raise ValueError('inactive or unrelated object')
            state = diagnostic(rewrite.get(frame, {}))
            if not state:
                continue
            if cls == 'GrowFlowerPot':
                if (source['sensors'] != [dict(Name='Body', Radius=70., Y=50.)]
                        or (obj['pending_n_name'] or obj['current_n_name']) != 'NoSeed'
                        or obj['pos_bits'] != initial[0]['pos_bits'] or obj['quat_bits'] != initial[0]['quat_bits']
                        or obj['velocity_bits'] != '00000000,00000000,00000000'):
                    raise ValueError('pot is not the exported stationary source')
                expected = words([f32(a+b) for a,b in zip(values(velocity, 'push_min'), values(velocity, 'push_max'))])
                if state.get('push') != expected:
                    continue
                if not exact_vector(state.get('velocity', ''), velocity['velocity_before_bits']):
                    continue
                if not exact_vector(state.get('result', ''), velocity['velocity_result_bits']):
                    continue
                if any(x != 0. for x in values(velocity, 'push_min') + values(velocity, 'push_max')):
                    pushing.add(frame)
            else:
                nerve = keeper['keeper_pending_n_name'] or keeper['keeper_current_n_name']
                if nerve not in ('Wait', 'Start', 'Carry', 'Throw'):
                    continue
                held = keeper['held_id'] == source['id']
                phase = {'Wait': 0, 'Start': 1, 'Carry': 2, 'Throw': 3}[nerve]
                if state.get('keeper', '').split('/')[::2] != [str(phase), str(int(held))]:
                    continue
                if phase and state.get('keeper') != f"{phase}/{keeper['keeper_step']}/{int(held)}":
                    continue
                if held and (keeper['overhead'] != '0' or keeper['wall_keep'] != '1'
                             or 'held' not in state or not state.get('seed', '').startswith(source['id'] + '/')):
                    continue
                if phase == 3 and held and state.get('inhibition') != keeper['inhibition']:
                    continue
                if frame in attachments:
                    attachment = attachments[frame]
                    previous_keeper = keepers.get(frame-1, {})
                    throw_hand = (previous_keeper.get('keeper_pending_n_name') or
                                  previous_keeper.get('keeper_current_n_name')) == 'Throw'
                    if not exact_vector(words(hold_matrix(attachment, throw_hand)), attachment['hold_matrix_bits']):
                        raise ValueError('original attachment does not follow the hold-matrix equation')
                    previous = diagnostic(rewrite.get(frame-1, {}))
                    pose = previous.get('held', '').split('/')
                    if held and (len(pose) != 12 or state.get('seed') != source['id'] + '/' + '/'.join(pose[i] for i in (3,7,11))):
                        continue
                if frame in self.required and not held:
                    if not seed_motion_matches(state, obj, source['id']):
                        continue
                if frame in self.required:
                    released = keepers.get(frame-1, {}).get('held_id') == source['id'] and not held
                    release_events = [e for e in events[frame] if e['kind'] == 'object_receive_after'
                                      and 'Release' in e['message_type']]
                    if released:
                        if len(release_events) != 1:
                            continue
                        e = release_events[0]
                        if (state.get('keeper') != '3/6/0' or e['message_type'] != 'al::SensorMsgPlayerRelease'
                                or e['answer'] != '1' or e['receiver_id'] != source['id']
                                or e['receiver_actor'] != token or e['sender_id'] != 'Mario'
                                or e['keeper_current_n_name'] != 'Throw' or e['keeper_step'] != '5'):
                            continue
                    elif release_events:
                        continue
                if obj['hold_collider_push'] != '0' or obj['collision_parts_present'] != '0':
                    raise ValueError('unsupported seed collision composition')
                if values(obj,'hold_offset') != [0.,-30.,40.] or values(obj,'hold_rotation') != [0.,0.,0.]:
                    raise ValueError('unexpected per-object hold transform')
            valid = True
            for event in events[frame]:
                if event['kind'] != 'mario_receive_after':
                    continue
                if event['sender_id'] != source['id'] or event['sender_actor'] != token:
                    valid = False
                    break
                answer = mario_answer(event['message_type'], event['receiver_sensor'])
                if answer is None or str(int(answer)) != event['answer']:
                    valid = False
                    break
                if answer and (cls != 'GrowFlowerPot' or event['sender_sensor'] != 'Body'
                               or event['receiver_sensor'] != 'Body'):
                    valid = False
                    break
            if valid:
                frames.add(frame)
        self.frames, self.pushing, self.identity = frames, pushing, identity

    def contact(self, frame, contact):
        if frame not in self.frames or not self.identity or contact['who'] != 'player':
            return False
        token, cls, host = self.identity
        if (contact['actor'], contact['host_class'], contact['host']) in self.passive:
            return contact['kind'] == 'a' and contact['other'] == 'SeedNear'
        if (contact['actor'], contact['host_class'], contact['host']) != (token, cls, host):
            return False
        if contact['kind'] in ('r', 'p'):
            answer = mario_answer(contact['message'], contact['self'])
            return answer is not None and contact['answer'] == str(int(answer))
        return contact['kind'] == 'a'


def seed_motion_matches(state, obj, identity):
    """No numeric margin: validate the independently simulated release/flight/rest/hidden seed."""
    if not all(state.get(k) for k in ('seed', 'seed_velocity', 'seed_quat', 'seed_nerve', 'seed_collide_off')):
        return False
    seed = state.get('seed', '').split('/', 1)
    if len(seed) != 2 or seed[0] != identity:
        return False
    if not exact_vector(seed[1], obj['pos_bits']):
        return False
    if not exact_vector(state.get('seed_velocity', ''), obj['velocity_bits']):
        return False
    if not exact_vector(state.get('seed_quat', ''), obj['quat_bits']):
        return False
    nerve = obj['pending_n_name'] or obj['current_n_name']
    step = '0' if obj['pending_n_name'] else obj['step']
    return (state.get('seed_nerve') == f'{nerve}/{step}'
            and state.get('seed_collide_off') == obj['collide_off'])


def self_check():
    assert mario_answer('rs::SensorMsgHitGrowFlowerPot', 'Body') is False
    assert mario_answer('rs::SensorMsgUnknownFutureType', 'Body') is False
    assert mario_answer('rs::SensorMsgPushToPlayer', 'Body') is True
    assert mario_answer('rs::SensorMsgPushToPlayer', 'Carry') is False
    assert mario_answer('al::SensorMsgCollidePush', 'Body') is None
    assert not Evidence(None, None, None, None, None, None, None).frames
    state = dict(seed='obj2876/00000000/3f800000/40000000',
                 seed_velocity='00000000/00000000/00000000',
                 seed_quat='00000000/00000000/00000000/3f800000',
                 seed_nerve='Revival/1', seed_collide_off='1')
    obj = dict(pos_bits='00000000,3f800000,40000000',
               velocity_bits=state['seed_velocity'], quat_bits=state['seed_quat'],
               current_n_name='Revival', pending_n_name='', step='1', collide_off='1')
    assert seed_motion_matches(state, obj, 'obj2876')
    for key in state:
        changed = dict(state, **{key: state[key] + '1'})
        try:
            assert not seed_motion_matches(changed, obj, 'obj2876')
        except (ValueError, struct.error):
            pass  # Malformed words are also a refusal, never a numeric allowance.
    assert not seed_motion_matches(state, obj, 'another placement')


if __name__ == '__main__':
    self_check()
