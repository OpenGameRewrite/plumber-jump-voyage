"""Placement-bound CapTrampoline proof; passive side files validate the native source."""
import math
import struct
from collections import defaultdict
from sand_geyser import f32, bits
from carry_objects import exact_vector, mario_answer


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
    def __init__(self, closed=False):
        self.id = 'obj1043' if closed else 'obj1088'
        self.phase, self.step, self.cooldown, self.close = (0, 0, 0, 300) if closed else (1, -1, 0, 300)
        self.initial = not closed
        self.continue_open = not closed

    def reflect(self, position, feet):
        # receiveMsg 0x24362c..0x2436ec; InitSensor radii/offsets.
        foot = [feet[0], f32(feet[1]+40.), feet[2]]
        centre = [position[0], f32(position[1]+60.), position[2]]
        accepted = (self.phase == 0 and self.cooldown <= 0
                    and overlap(foot, centre, 150.) and foot[1] > f32(centre[1]-100.))
        if accepted:
            self.phase, self.step, self.cooldown, self.close = 3, -1, 10, 300
        return accepted

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
        if self.phase == 3 and self.step == 15:
            self.phase, self.step = 1, -1
        if self.phase == 1 and not self.continue_open and self.step >= 0:
            self.close -= 1  # exeOpen 0x243b24..0x243b34
        self.step += 1

    def diagnostic(self):
        return f'{self.id}/{self.phase}/{self.step}/{self.cooldown}/{self.close}'


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
        closed = p['Id'] == 'obj1043'
        phase = 'WaitPending' if closed else 'OpenPending'
        position = dict(X=400., Y=0., Z=-600.) if closed else dict(X=3800., Y=700., Z=-6200.)
        rotation = dict(X=0., Y=0., Z=0.) if closed else dict(X=0., Y=90., Z=0.)
        if (sources[0].get('initial_phase') != phase
                or p['Id'] not in ('obj1088', 'obj1043') or p['UnitConfigName'] != 'CapTrampoline'
                or p['Translate'] != position
                or p['Rotate'] != rotation
                or p['Scale'] != dict(X=1., Y=1., Z=1.) or p['Links']
                or p['IsConnectToCollision'] or p['IsLinkDest'] or p['IsMoon']
                or p['IsContinueOpen'] != (not closed) or p['IsShowTutorial']
                or sources[0]['sensors'] != [dict(Name=n, Radius=r, Type='MapObj', **({'Y': y} if y else {}))
                                            for n, (r, y) in SENSORS.items()]):
            raise ValueError('unsupported placement/sensors')
        position = [p['Translate'][a] for a in 'XYZ']
        samples, sensors, events = {}, defaultdict(dict), defaultdict(list)
        initial = [r for r in actors if r['id'] == p['Id'] and r['kind'] == 'init']
        if len(initial) != 1 or initial[0]['step'] != ('0' if closed else '-1') or initial[0]['pending_n'] != ('1ce4ea0' if closed else '1ce4ea8'):
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
        source = Source(closed)
        identity = None
        frames, allowed = set(), set()
        power, stay, spinning = 0., 0, False
        reflects = set()
        for f in range(first, last+1):
            r, m, g = samples[f], players[f], original[f]
            previous = original.get(f-1)
            feet = vector(previous['bits_pos']) if previous else None
            velocity = vector(previous['bits_vel']) if previous else None
            request = source.attack(position, feet, velocity) if previous else False
            source.tick()
            wait_step = source.step
            reflect = (source.reflect(position, feet) if previous and previous['state'] == 'HipDrop/Fall' else False)
            if spinning and not request and previous.get('ground') == '1':
                # exeJumpSpinFlower 0x472fd8..0x472ff4 ends the child;
                # exeJump 0x4240a4..0x4240d8 counts/clears then setNerveOnGround.
                if g['state'] != 'Wait/Land' or g['counters'].split('|')[0] != 'cjump=0/10':
                    raise ValueError(f'spin grounded exit {f}')
                spinning = False
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
            nerve = {0:'1ce4ea0', 1:'1ce4ea8', 2:'1ce4eb0', 3:'1ce4eb8'}[source.phase]
            if (r['pending_n'] or '0') not in ('0', nerve) or (r['pending_n'] if r['pending_n'] != '0' else r['current_n']) != nerve:
                raise ValueError(f'nerve {f}')
            if (int(r['step']) != source.step or int(r['cooldown']) != source.cooldown
                    or int(r['close_counter']) != source.close
                    or any(r[k] != '0' for k in ('connector', 'dead', 'clipped', 'collision_parts', 'moon', 'tutorial'))
                    or r['continue_open'] != str(int(not closed)) or r['camera_arg'] != '1'
                    or not exact_vector(r['pos_bits'], ','.join(bits(v) for v in position))
                    or not exact_vector(r['velocity_bits'], '00000000,00000000,00000000')
                    or r['quat_bits'] != ('00000000,00000000,00000000,3f800000' if closed else '00000000,3f3504f2,00000000,3f3504f3')):
                raise ValueError(f'source state {f}')
            # Original CapTrampoline.bfres: Wait2, Jump19, OpenStart15, Open46. Nerve changes retain the
            # completed action until the new nerve's first execution.
            animation, animation_frame = (
                ('Wait', (wait_step-1) % 2) if source.phase == 0 or reflect else
                ('OpenStart', source.step-1) if source.phase == 3 else
                ('Jump', source.step-1) if source.phase == 2 else
                ('Wait', 0) if source.step == -1 else
                ('OpenStart', 15) if closed and source.step == 0 else
                ('Jump', 19) if source.step == 0 else ('Open', (source.step-1) % 46))
            if (r['anim'] != animation or r['anim_frame_bits'] != bits(animation_frame)
                    or r['anim_rate_bits'] != bits(1.)
                    or r['counter118'] != '0' or r['flag120'] != '0'):
                raise ValueError(f'source action {f}')
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
                             or m['jump_b4'] != '0' or m['jump_b6'] != '0' or g['state'] != 'Jump/JumpSpinFlower'):
                raise ValueError(f'spin launch/stay state {f}')
            if reflect:
                reflects.add(f)
                self.verify_reflect(f, events[f], p, identity, m, g)
            elif any(e['kind'] == 'object_receive_after' and e['answer'] == '1'
                     and e['message_type'] == 'al::SensorMsgPlayerAttackObjHipDropReflect' for e in events[f]):
                raise ValueError('unexpected accepted Reflect')
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
                expected = (own not in ('Carry', 'HipDropKnockDown')
                            if msg == 'rs::SensorMsgRequestPlayerSpinJump' else mario_answer(msg, own))
                if expected is None:
                    raise ValueError('unported Mario dispatch')
                if e['answer'] != str(int(expected)):
                    raise ValueError('unproved Mario answer')
                if expected and (not request or own != 'Foot' or other != 'TrampleHit'):
                    raise ValueError('unexpected accepted request')
                if msg not in ('al::SensorMsgPush', 'rs::SensorMsgPushToFish', 'rs::SensorMsgPushToPlayer', 'rs::SensorMsgRequestPlayerSpinJump', 'rs::SensorMsgRequestSphinxJump', 'rs::SensorMsgCapTrampolineAttack'):
                    raise ValueError('unproved Mario message type')
                allowed.add((f, own, other, msg, e['answer']))
            if closed and previous:
                # ActorHitReactionCtrl/StompHit StopSceneFrame2; HitReactionKeeper
                # 0x90cb80..0x90cb8c supplies delay2, SceneStopCtrl 0x9cf150.
                if int(g['game_frame']) - int(previous['game_frame']) != (3 if f-2 in reflects else 1):
                    raise ValueError('unproved pad/scene-stop clock')
            if power and (m['jump_launch_bits'] != bits(power) or int(m['stay_counter']) != stay
                          or m['jump_b4'] != '0' or m['jump_b6'] != '0'):
                raise ValueError('retained spin launch/exit fields')
            if f >= 0:
                if rewrite[f].get('trampoline_source', '') != source.diagnostic() + f';jump={bits(power)}/{stay}':
                    raise ValueError(f'native source {f}')
                frames.add(f)
                if feet:
                    self.previous[f] = feet
        self.frames, self.identity, self.messages = frames, (p['UnitConfig']['DisplayName'], *identity), allowed
        self.position = position

    @staticmethod
    def verify_reflect(frame, events, placement, identity, player, game):
        # Keep original receive/send answers distinct from Mario's later request6.
        msg = 'al::SensorMsgPlayerAttackObjHipDropReflect'
        receives = [e for e in events if e['kind'] == 'object_receive_after' and e['message_type'] == msg]
        sends = [e for e in events if e['kind'] == 'send_after' and e['message_type'] == msg]
        if len(receives) != 2 or len(sends) != 2:
            raise ValueError('Reflect dispatch count')
        for group in (receives, sends):
            if [(e['receiver_sensor'], e['answer']) for e in group] != [('CapHit', '1'), ('Attack', '0')]:
                raise ValueError('Reflect original answers/order')
            for e in group:
                if (e['sender_id'] != 'Mario' or e['sender_sensor'] != 'Foot'
                        or e['receiver_id'] != placement['Id'] or e['receiver_actor'] != identity[0]
                        or e['receiver_generation'] != identity[1]):
                    raise ValueError('Reflect identity')
        ordered = [events.index(e) for e in (receives[0], sends[0], receives[1], sends[1])]
        if ordered != sorted(ordered):
            raise ValueError('Reflect receive/send nesting')
        appears = [e for e in events if e['kind'] == 'jump_appear_after']
        if len(appears) != 1 or appears[0]['request_type'] != '6' or appears[0]['attack3'] != '1':
            raise ValueError('Reflect Mario request/trigger')
        if events.index(appears[0]) <= ordered[-1]:
            raise ValueError('Reflect appear ordering')
        if (game['state'] != 'Jump/Jump' or player['jump_launch_bits'] != bits(35.)
                or player['jump_a8_bits'] != bits(1.5) or player['jump_b6'] != '0'):
            raise ValueError('Reflect launch')

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

    closed = Source(True)
    assert not closed.reflect([0., 0., 0.], [0., -80., 0.])
    assert closed.reflect([0., 0., 0.], [0., 73., 0.])
    assert (closed.phase, closed.step, closed.cooldown) == (3, -1, 10)
    assert not closed.reflect([0., 0., 0.], [0., 73., 0.])
    for _ in range(16):
        closed.tick()
    assert (closed.phase, closed.step, closed.cooldown, closed.close) == (1, 0, 0, 300)
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
        nerve = {0:'1ce4ea0', 1:'1ce4ea8', 2:'1ce4eb0', 3:'1ce4eb8'}[source.phase]
        actors.append(dict(frame=str(frame), id='obj1088', kind='sample', actor='10',
                           generation='3', zone='', registry_dropped='0',
                           current_n='0' if frame == -1 else nerve,
                           pending_n=nerve if frame == -1 else '0', step=str(source.step),
                           cooldown=str(source.cooldown), close_counter='300', connector='0',
                           dead='0', clipped='0', collision_parts='0', moon='0', tutorial='0',
                           continue_open='1', camera_arg='1',
                           anim='Wait' if frame == -1 else 'Jump',
                           anim_frame_bits=bits(max(0, source.step-1)),
                           anim_rate_bits=bits(1.), counter118='0', flag120='0',
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
                            stay_counter='0' if frame == -1 else '80', jump_b4='0', jump_b6='0'))
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
