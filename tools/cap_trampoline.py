"""Placement-bound CapTrampoline proof; passive side files validate the native source."""
import math
import struct
from collections import defaultdict
from sand_geyser import f32, bits
from carry_objects import exact_vector


SENSORS = {'Body': (76., 0.), 'CapHit': (110., 60.),
           'TrampleHit': (120., -20.), 'Attack': (200., 0.)}
PLAYER = {('Foot', '3'): (40., 40.), ('Body', '1'): (50., 75.),
          ('Head', '1'): (40., 110.), ('Carry', '2'): (130., 100.),
          ('Eye', '5'): (800., 80.), ('HipDropKnockDown', '2'): (150., 0.)}


def vector(cell):
    return [struct.unpack('>f', bytes.fromhex(x))[0] for x in cell.replace('0x', '').replace('/', ',').split(',')]


def squared(v):
    return f32(f32(f32(v[0]*v[0]) + f32(v[1]*v[1])) + f32(v[2]*v[2]))


def overlap(a, b, radius):
    return squared([f32(x-y) for x, y in zip(a, b)]) < f32(radius*radius)


class Source:
    def __init__(self):
        self.phase, self.step, self.cooldown, self.close = 1, -1, 0, 300
        self.initial = True

    def attack(self, position, feet, velocity):
        centre = [position[0], f32(position[1]-20.), position[2]]
        foot = [feet[0], f32(feet[1]+40.), feet[2]]
        delta = [f32(a-b) for a, b in zip(centre, foot)]
        toward = f32(f32(f32(velocity[0]*delta[0])+f32(velocity[1]*delta[1]))+f32(velocity[2]*delta[2]))
        accepted = (self.phase == 1 and self.cooldown <= 0
                    and squared(delta) < 160.*160.
                    and f32(math.sqrt(squared(velocity))) >= f32(1.45) and toward >= 0.)
        if accepted:
            self.phase, self.step, self.cooldown, self.close = 2, -1, 10, 300
        return accepted

    def tick(self):
        if self.initial:
            self.initial = False
            return
        self.cooldown = max(0, self.cooldown-1)
        self.step = max(0, self.step)
        if self.phase == 2 and self.step == 19:
            self.phase, self.step = 1, -1
        self.step += 1

    def diagnostic(self):
        return f'obj1088/{self.phase}/{self.step}/{self.cooldown}/{self.close}'


class Evidence:
    def __init__(self, geometry, actors, player, messages, rewrite, rows, preroll):
        self.frames, self.identity, self.messages, self.previous = set(), None, set(), {}
        self.error = None
        try:
            self.verify(geometry, actors, player, messages, rewrite, rows, preroll)
        except (ValueError, KeyError, TypeError, IndexError, StopIteration, OverflowError, struct.error) as error:
            self.frames, self.identity, self.messages = set(), None, set()
            self.error = str(error)

    def verify(self, geometry, actors, player, messages, rewrite, rows, preroll):
        if not all((geometry, actors, player, messages, rewrite, rows, preroll)):
            return
        if (geometry['stage'], geometry['scenario']) != ('TrampolineWallCatchExStage', 1):
            return
        sources = geometry.get('cap_trampolines', [])
        if len(sources) != 1:
            return
        p = sources[0]['placement']
        if (sources[0].get('initial_phase') != 'OpenPending'
                or p['Id'] != 'obj1088' or p['UnitConfigName'] != 'CapTrampoline'
                or p['Translate'] != dict(X=3800., Y=700., Z=-6200.)
                or p['Rotate'] != dict(X=0., Y=90., Z=0.)
                or p['Scale'] != dict(X=1., Y=1., Z=1.) or p['Links']
                or p['IsConnectToCollision'] or p['IsLinkDest'] or p['IsMoon']
                or not p['IsContinueOpen'] or p['IsShowTutorial']
                or sources[0]['sensors'] != [dict(Name=n, Radius=r, Type='MapObj', **({'Y': y} if y else {}))
                                            for n, (r, y) in SENSORS.items()]):
            raise ValueError('unsupported placement/sensors')
        position = [p['Translate'][a] for a in 'XYZ']
        samples, sensors, events = {}, defaultdict(dict), defaultdict(list)
        initial = [r for r in actors if r['id'] == p['Id'] and r['kind'] == 'init']
        if len(initial) != 1 or initial[0]['step'] != '-1' or initial[0]['pending_n'] != '1ce4ea8':
            raise ValueError('initial source phase')
        for r in actors:
            if r['id'] != p['Id'] or int(r['frame']) == -2147483648:
                continue
            f = int(r['frame'])
            if r['kind'] == 'sample':
                if f in samples:
                    raise ValueError('duplicate actor frame')
                samples[f] = r
            elif r['kind'] == 'sensor':
                if r['sensor'] in sensors[f]:
                    raise ValueError('duplicate sensor')
                sensors[f][r['sensor']] = r
        players = {int(r['frame']): r for r in player}
        if len(players) != len(player):
            raise ValueError('duplicate player frame')
        for r in messages:
            events[int(r['frame'])].append(r)
        original = {**preroll, **rows}
        first, last = min(samples), max(samples)
        source = Source()
        identity = None
        frames, allowed = set(), set()
        power, stay, spinning = 0., 0, False
        for f in range(first, last+1):
            r, m, g = samples[f], players[f], original[f]
            previous = original.get(f-1)
            feet = vector(previous['bits_pos']) if previous else None
            velocity = vector(previous['bits_vel']) if previous else None
            request = source.attack(position, feet, velocity) if previous else False
            source.tick()
            if request:
                power, spinning = 60., True
            if spinning:
                if request or (velocity[1] >= 0. and abs(velocity[1]) >= f32(.001)):
                    stay = 80
                elif stay > 0:
                    stay -= 1
            current = (r['actor'], r['generation'])
            identity = identity or current
            if current != identity or r['zone'] or r['registry_dropped'] != '0':
                raise ValueError('identity/drop')
            nerve = '1ce4ea8' if source.phase == 1 else '1ce4eb0'
            if (r['pending_n'] or '0') not in ('0', nerve) or (r['pending_n'] if r['pending_n'] != '0' else r['current_n']) != nerve:
                raise ValueError(f'nerve {f}')
            if (int(r['step']) != source.step or int(r['cooldown']) != source.cooldown
                    or int(r['close_counter']) != source.close
                    or any(r[k] != '0' for k in ('connector', 'dead', 'clipped', 'collision_parts', 'moon', 'tutorial'))
                    or r['continue_open'] != '1' or r['camera_arg'] != '1'
                    or not exact_vector(r['pos_bits'], ','.join(bits(v) for v in position))
                    or not exact_vector(r['velocity_bits'], '00000000,00000000,00000000')
                    or r['quat_bits'] != '00000000,3f3504f2,00000000,3f3504f3'):
                raise ValueError(f'source state {f}')
            if set(sensors[f]) != set(SENSORS):
                raise ValueError('source sensor set')
            for name, (radius, y) in SENSORS.items():
                s = sensors[f][name]
                centre = [position[0], f32(position[1]+y), position[2]]
                if (s['valid'] != '1' or s['system'] != '1' or s['type'] != '12'
                        or s['radius_bits'] != bits(radius)
                        or not exact_vector(s['centre_bits'], ','.join(bits(v) for v in centre))):
                    raise ValueError(f'source sensor {f}')
            if (m['dropped'] != '0' or not exact_vector(m['player_pos_bits'], g['bits_pos'].replace('0x', ''))
                    or not exact_vector(m['player_velocity_bits'], g['bits_vel'].replace('0x', ''))):
                raise ValueError(f'player side validation {f}')
            if spinning and (m['jump_launch_bits'] != bits(power) or int(m['stay_counter']) != stay
                             or m['jump_b6'] != '0' or g['state'] != 'Jump/JumpSpinFlower'):
                raise ValueError(f'spin launch/stay state {f}')
            accepted = [e for e in events[f] if e['kind'] == 'spin_request_after' and e['answer'] == '1']
            if len(accepted) != int(request):
                raise ValueError(f'request schedule {f}')
            for e in accepted:
                if (e['sender_id'] != p['Id'] or e['sender_actor'] != identity[0]
                        or e['sender_generation'] != identity[1] or e['sender_sensor'] != 'TrampleHit'
                        or e['receiver_id'] != 'Mario' or e['receiver_sensor'] != 'Foot'
                        or e['power_bits'] != bits(60.) or e['request_type'] != '13'
                        or e['request_power_bits'] != bits(60.)):
                    raise ValueError('request identity/power')
            for e in events[f]:
                if e['kind'] != 'mario_receive_after':
                    continue
                if e['sender_id'] != p['Id'] or e['sender_actor'] != identity[0] or e['sender_generation'] != identity[1]:
                    raise ValueError('unbound message sender')
                msg, own, other = e['message_type'], e['receiver_sensor'], e['sender_sensor']
                expected = (own not in ('Carry', 'HipDropKnockDown') and msg == 'rs::SensorMsgRequestPlayerSpinJump')
                if e['answer'] != str(int(expected)):
                    raise ValueError('unproved Mario answer')
                if expected and (not request or own != 'Foot' or other != 'TrampleHit'):
                    raise ValueError('unexpected accepted request')
                if msg not in ('al::SensorMsgPush', 'rs::SensorMsgPushToFish', 'rs::SensorMsgPushToPlayer', 'rs::SensorMsgRequestPlayerSpinJump', 'rs::SensorMsgRequestSphinxJump', 'rs::SensorMsgCapTrampolineAttack'):
                    raise ValueError('unproved Mario message type')
                allowed.add((f, own, other, msg, e['answer']))
            if f >= 0:
                if rewrite[f].get('trampoline_source', '') != source.diagnostic() + f';jump={bits(power)}/{stay}':
                    raise ValueError(f'native source {f}')
                frames.add(f)
                if feet:
                    self.previous[f] = feet
        self.frames, self.identity, self.messages = frames, (p['UnitConfig']['DisplayName'], *identity), allowed
        self.position = position

    def contact(self, frame, c):
        if frame not in self.frames or not self.identity:
            return False
        host, token, _generation = self.identity
        if (c['who'] != 'player' or c['host_class'] != 'CapTrampoline'
                or c['host'] != host or c['actor'] != token or c['other_type'] != '12'
                or c['other'] not in SENSORS):
            return False
        own = PLAYER.get((c['self'], c['self_type']))
        feet = self.previous.get(frame)
        if not own or feet is None:
            return False
        radius, y = SENSORS[c['other']]
        point = [feet[0], f32(feet[1]+own[1]), feet[2]]
        centre = [self.position[0], f32(self.position[1]+y), self.position[2]]
        if not overlap(point, centre, own[0]+radius):
            return False
        return ((c['kind'] == 'a' and not c['message'] and not c['answer'])
                # receivePushMsg only dispatches push/collision identities (0x4293b0);
                # this request falls through false before receiveMsg accepts it.
                or (c['kind'] == 'p' and c['message'] == 'rs::SensorMsgRequestPlayerSpinJump'
                    and c['answer'] == '0' and (frame, c['self'], c['other'], c['message'], '1') in self.messages)
                or (c['kind'] == 'r' and (frame, c['self'], c['other'], c['message'], c['answer']) in self.messages))


def self_check():
    from copy import deepcopy

    source = Source()
    source.initial = False
    assert not source.attack([0., 0., 0.], [0., 100., 0.], [0., -45., 0.])
    assert source.attack([0., 0., 0.], [0., 80., 0.], [0., -45., 0.])
    source.tick()
    assert (source.phase, source.step, source.cooldown) == (2, 1, 9)
    assert not source.attack([0., 0., 0.], [0., 80., 0.], [0., -45., 0.])
    for _ in range(19):
        source.tick()
    assert (source.phase, source.step, source.cooldown) == (1, 0, 0)
    assert not Evidence(None, None, None, None, {}, {}, {}).frames
    placement = dict(Id='obj1088', UnitConfigName='CapTrampoline',
                     Translate=dict(X=3800., Y=700., Z=-6200.),
                     Rotate=dict(X=0., Y=90., Z=0.), Scale=dict(X=1., Y=1., Z=1.),
                     Links={}, IsConnectToCollision=False, IsLinkDest=False,
                     IsMoon=False, IsContinueOpen=True, IsShowTutorial=False,
                     UnitConfig=dict(DisplayName='trampoline'))
    geometry = dict(stage='TrampolineWallCatchExStage', scenario=1, cap_trampolines=[dict(
        placement=placement, initial_phase='OpenPending', sensors=[
            dict(Name=n, Radius=r, Type='MapObj', **({'Y': y} if y else {}))
            for n, (r, y) in SENSORS.items()])])
    actors = [dict(id='obj1088', kind='init', frame='-2147483648', step='-1', pending_n='1ce4ea8')]
    players, rows, rewrite = [], {}, {}
    source = Source()
    pos = [3800., 700., -6200.]
    feet = [3800., 780., -6200.]
    for frame in (-1, 0, 1):
        if frame == 0:
            assert source.attack(pos, feet, [0., -45., 0.])
        source.tick()
        nerve = '1ce4ea8' if source.phase == 1 else '1ce4eb0'
        actors.append(dict(frame=str(frame), id='obj1088', kind='sample', actor='10',
                           generation='3', zone='', registry_dropped='0',
                           current_n='0' if frame == -1 else nerve,
                           pending_n=nerve if frame == -1 else '0', step=str(source.step),
                           cooldown=str(source.cooldown), close_counter='300', connector='0',
                           dead='0', clipped='0', collision_parts='0', moon='0', tutorial='0',
                           continue_open='1', camera_arg='1',
                           pos_bits=','.join(bits(v) for v in pos),
                           velocity_bits='00000000,00000000,00000000',
                           quat_bits='00000000,3f3504f2,00000000,3f3504f3'))
        for name, (radius, y) in SENSORS.items():
            actors.append(dict(frame=str(frame), id='obj1088', kind='sensor', sensor=name,
                               valid='1', system='1', type='12', radius_bits=bits(radius),
                               centre_bits=','.join(bits(v) for v in [pos[0], pos[1]+y, pos[2]])))
        velocity = [0., -45. if frame == -1 else 58.5, 0.]
        rows[frame] = dict(bits_pos=','.join(bits(v) for v in feet),
                           bits_vel=','.join(bits(v) for v in velocity),
                           state='Wait/Wait' if frame == -1 else 'Jump/JumpSpinFlower')
        players.append(dict(frame=str(frame), dropped='0', player_pos_bits=rows[frame]['bits_pos'],
                            player_velocity_bits=rows[frame]['bits_vel'], jump_launch_bits=bits(60.),
                            stay_counter='0' if frame == -1 else '80', jump_b6='0'))
        if frame >= 0:
            rewrite[frame] = dict(trampoline_source=source.diagnostic() + ';jump=42700000/80')
    event = dict(frame='0', sender_id='obj1088', sender_actor='10', sender_generation='3',
                 sender_sensor='TrampleHit', receiver_id='Mario', receiver_sensor='Foot',
                 answer='1', power_bits=bits(60.), request_type='13', request_power_bits=bits(60.))
    messages = [dict(event, kind='spin_request_after'), dict(event, kind='mario_receive_after',
                message_type='rs::SensorMsgRequestPlayerSpinJump')]
    args = [geometry, actors, players, messages, rewrite,
            {f: r for f, r in rows.items() if f >= 0}, {-1: rows[-1]}]
    evidence = Evidence(*args)
    assert evidence.frames == {0, 1}, evidence.error
    contact = dict(who='player', kind='r', host_class='CapTrampoline', host='trampoline',
                   actor='10', other='TrampleHit', other_type='12', self='Foot', self_type='3',
                   message='rs::SensorMsgRequestPlayerSpinJump', answer='1')
    assert evidence.contact(0, contact)
    assert evidence.contact(0, dict(contact, kind='p', answer='0'))
    assert not evidence.contact(0, dict(contact, actor='11'))
    assert not evidence.contact(0, dict(contact, answer='0'))
    for change in ('power', 'sensor', 'clock', 'native', 'stay', 'stage'):
        bad = deepcopy(args)
        if change == 'power':
            bad[3][0]['power_bits'] = bits(68.)
        elif change == 'sensor':
            bad[1][2]['centre_bits'] = '00000000,00000000,00000000'
        elif change == 'clock':
            bad[1][1]['step'] = '99'
        elif change == 'native':
            bad[4][1]['trampoline_source'] = 'obj1088/1/0/0/300;jump=42700000/80'
        elif change == 'stay':
            bad[2][1]['stay_counter'] = '79'
        else:
            bad[0]['scenario'] = 2
        assert not Evidence(*bad).frames, change


if __name__ == '__main__':
    self_check()
