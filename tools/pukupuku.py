"""The placed cave Pukupuku's closed Bezier rail (1.0.0).

Only original placement/animation data and an initial source phase are inputs.
Recorded subsequent samples are verification evidence, never source positions.
"""
import math
import struct


def f32(x):
    return struct.unpack('<f', struct.pack('<f', x))[0]


def bits(x):
    return struct.pack('>f', x).hex()


def vector_bits(v):
    return ','.join(bits(x) for x in v)


class Bezier:
    """al::BezierCurve: power coefficients, quantized Simpson length, Newton inverse."""
    def __init__(self, points):
        self.c = []
        for p0, p1, p2, p3 in zip(*points):
            a, b, c = f32(p1-p0), f32(p2-p1), f32(p3-p2)
            self.c.append([p0, f32(a*3), f32(f32(b-a)*3), f32(f32(c-b)-f32(b-a))])
        self.length = self.arc(1.)

    def speed(self, t):
        twice, cube = f32(t+t), f32(t*f32(t*3.))
        v = [f32(f32(f32(twice*c)+b)+f32(cube*d)) for _, b, c, d in self.c]
        return f32(math.sqrt(f32(f32(f32(v[0]*v[0])+f32(v[1]*v[1]))+f32(v[2]*v[2]))))

    def arc(self, t):
        h = f32(t*f32(1./20.))
        endpoints = f32(f32(self.speed(0.)+self.speed(t))*.5)
        even, odd = 0., 0.
        for i in range(1, 11):
            odd = f32(odd+self.speed(f32(h*f32(float(2*i)-1.))))
            if i != 10:
                even = f32(even+self.speed(f32(h*float(2*i))))
        result = f32(f32(h*f32(1./3.))*f32(f32(endpoints+f32(even+even))+f32(odd*4.)))
        return f32(math.floor(f32(result*1024.))*f32(1./1024.))

    def parameter(self, distance):
        t = f32(distance/self.length)
        length = self.arc(t)
        for _ in range(5):
            if abs(f32(distance-length)) <= f32(.01):
                break
            t = min(1., max(0., f32(t+f32(f32(distance-length)/max(self.speed(t), f32(.001))))))
            length = self.arc(t)
        return t

    def position(self, distance):
        t = self.parameter(distance)
        t2, t3 = f32(t*t), f32(f32(t*t)*t)
        return [f32(f32(f32(f32(b*t)+a)+f32(t2*c))+f32(t3*d)) for a, b, c, d in self.c]


def rail_curves(placement):
    points = placement['rail']
    return [Bezier([points[i]['position'], points[i]['out'], points[1-i]['in'], points[1-i]['position']]) for i in range(2)]


class Source:
    def __init__(self, placement):
        self.curves = rail_curves(placement)
        self.length = f32(self.curves[0].length+self.curves[1].length)
        self.coord = 0.

    def position(self):
        first = self.curves[0].length
        curve, distance = (self.curves[0], self.coord) if self.coord <= first else (self.curves[1], f32(self.coord-first))
        return curve.position(distance)

    def tick(self):
        self.coord = f32(self.coord+5.)
        if self.coord >= self.length:
            self.coord = f32(self.coord-self.length)


class Evidence:
    """Placement, independent rail phase and complete ordered message evidence.

    Other fish, accepted damage/push and incomplete records never gain a physical
    exemption. Eye queries and rejected inactive attacks are checked separately.
    """
    def __init__(self, geometry, side, rewrite, rows):
        self.overlaps, self.actor = set(), None
        try:
            self.verify(geometry, side, rewrite, rows)
        except (KeyError, ValueError, TypeError, IndexError, StopIteration, OverflowError):
            self.overlaps, self.actor = set(), None

    def verify(self, geometry, side, rewrite, rows):
        if not geometry or not side or not rows or (geometry.get('stage'), geometry.get('scenario')) != ('SeaWorldUtsuboCaveStage', 1):
            return
        placement = geometry['pukupuku']
        if (placement['id'], placement['rail_id'], placement['move_type'], placement['revive']) != ('obj473', 'rail15', 0, True):
            raise ValueError('unsupported source identity')
        source = Source(placement)
        samples = {int(r['frame']): r for r in side if r['kind'] == 'sample' and r['id'] == 'obj473'}
        first, last = min(samples), max(samples)
        if first >= 0 or set(samples) != set(range(first, last+1)) or set(rows) != set(range(last+1)):
            raise ValueError('incomplete source coverage')
        seed = samples[first]
        if seed['step'] != '1' or seed['anim_frame_bits'] != bits(0.) or seed['n_vt'] != '1ca58b8':
            raise ValueError('unsupported initial phase')
        source.coord = float(seed['coord'])
        # The general contact probe assigns tokens lazily; an initially
        # unreferenced actor has token0 until its first overlap of any kind.
        tokens = {r['actor'] for r in samples.values()} - {'0'}
        if len(tokens) > 1:
            raise ValueError('placement changes contact identity')
        actor, generation = next(iter(tokens), '0'), seed['generation']
        if seed['coord_bits'] != bits(source.coord) or seed['length_bits'] != bits(source.length):
            raise ValueError('source rail seed')
        by_frame = {}
        for r in side:
            by_frame.setdefault(int(r['frame']), []).append(r)
            if r['kind'] == 'sample' and (r['dropped'] != '0' or r['registry_dropped'] != '0'):
                raise ValueError('lost source evidence')
        hits = []
        overlaps = set()
        phase, step, animation = 0, 1, 0
        press_length = int(placement['animation']['clips']['PressDown']['length'])
        wait_length = int(placement['animation']['clips']['SwimWait']['length'])
        pending = False
        for frame in range(first, last+1):
            entries = by_frame[frame]
            events = [r for r in entries if r['kind'] not in ('sample', 'sensor')]
            if [int(r['seq']) for r in events] != list(range(len(events))):
                raise ValueError('event order/gap')
            r = samples[frame]
            receives = [e for e in events if e['kind'] == 'receive_after' and e['result'] == '1']
            hit = [e for e in receives if e['msg_vt'] == '1de8df8']
            if hit:
                if len(hit) != 1 or phase != 0 or hits or hit[0]['id'] != 'obj473':
                    raise ValueError('unsupported physical message')
                answer = hit[0]
                index = events.index(answer)
                event = events[index-1]
                sequence = events[index-8:index+3]
                if [(e['kind'],e.get('msg_vt'),e.get('result')) for e in sequence if e['kind'].startswith('receive_')] != [
                        ('receive_before','1de8e48',''),('receive_after','1de8e48','0'),
                        ('receive_before','1d9d628',''),('receive_after','1d9d628','0'),
                        ('receive_before','1de8df8',''),('receive_after','1de8df8','1')]:
                    raise ValueError('Reflect then ObjHipDrop ordering')
                before = next(e for e in reversed(events[:index]) if e['kind'] == 'attack_before')
                after = next(e for e in events[index:] if e['kind'] == 'attack_after')
                if (before['player_sensor'],before['fish_sensor'],before['attack3'],after['attack3'],after['attack2']) != ('Foot','Body','0','1','0'):
                    raise ValueError('hip-drop trigger evidence')
                if (event['sender_sensor'],event['sender_type'],event['fish_sensor'],event['fish_type'],event['sender_player']) != ('Foot','3','Body','9','1'):
                    raise ValueError('attack sensor identities')
                sender = [float(event['sender_centre_'+a]) for a in 'xyz']
                centre = [float(event['fish_centre_'+a]) for a in 'xyz']
                if (event['sender_radius_bits'],event['fish_radius_bits'],event['fish_centre_bits']) != (bits(40.),bits(80.),r['centre_bits']):
                    raise ValueError('event-time Body sphere')
                if sum((a-b)**2 for a,b in zip(sender,centre)) >= 120.**2:
                    raise ValueError('no actual sphere overlap')
                if rows[frame]['state'] != 'Swim/SwimReflectDownFall' or rows[frame-1]['state'] != 'Swim/SwimHipDropLoop':
                    raise ValueError('trigger consequence')
                hits.append(frame)
                pending = True
            for answer in receives:
                if answer in hit:
                    continue
                event = events[events.index(answer)-1]
                if (event['id'],event['msg_vt'],event['fish_sensor']) != ('obj473','1de9988','Body') or not (phase != 0 or hit):
                    raise ValueError('unexpected accepted receive')
                eye = (event['sender_player'],event['sender_sensor'],event['sender_type']) == ('1','Eye','5')
                self_query = (event['sender'],event['sender_sensor'],event['sender_type'],event['sender_centre_bits']) == (actor,'Body','9',event['fish_centre_bits'])
                if not (eye or self_query):
                    raise ValueError('foreign PlayerDisregard receive')
            for event in events:
                if event['kind'] == 'attack_before' and event['id'] == 'obj473':
                    overlaps.add((frame,event['player_sensor'],event['fish_sensor']))
            expected_phase, expected_step = (1,-1) if hit else (phase,step)
            # Nerve identities are checked below via current/pending objects;
            # their vtables are also stable across the complete recording.
            expected_current = ['1ca6020','1ca60c0','1ca6030'][phase]
            if (r['actor'] not in ('0',actor) or r['generation'] != generation or r['zone'] or r['move_type'] != '0'
                    or r['dead'] != '0' or r['clipped'] != '0' or r['velocity_bits'] != '00000000,00000000,00000000'
                    or r['coord_bits'] != bits(source.coord) or r['length_bits'] != bits(source.length)
                    or r['pos_bits'] != vector_bits(source.position()) or r['rail_pos_bits'] != r['pos_bits']
                    or r['anim'] != ('SwimWait' if phase == 0 else 'PressDown') or r['anim_frame_bits'] != bits(float(animation))
                    or int(r['step']) != expected_step or r['current_n'] != expected_current
                    or r['pending_n'] != ('1ca60c0' if hit else '0')
                    or (r['sensor'],r['type'],r['radius_bits'],r['valid'],r['system']) != ('Body','9',bits(80.),'1','1')):
                raise ValueError(f'source phase/rail mismatch at {frame}')
            if frame >= 0:
                diagnostic = '/'.join([bits(source.coord), *[bits(x) for x in source.position()],str(expected_phase),str(expected_step)])
                if 'puku='+diagnostic+'/' not in rewrite[frame].get('collision',''):
                    raise ValueError('rewrite source not independently reproduced')
            if frame == first:
                continue
            if pending:
                phase, step, animation, pending = 1, 1, 0, False
            elif phase == 0:
                source.tick(); step += 1; animation = (animation+1)%wait_length
            elif phase == 1:
                step += 1; animation = min(animation+1,press_length)
                if animation == press_length:
                    phase, step = 2, 0
            else:
                step += 1
        self.overlaps, self.actor = overlaps, actor

    def contact(self, frame, c):
        if c['who'] != 'player' or c['host_class'] != 'Pukupuku' or c['actor'] != self.actor:
            return False
        if c['kind'] == 'a':
            return not c['message'] and not c['answer'] and (frame,c['self'],c['other']) in self.overlaps
        # These fish attacks reached Mario's non-body HipDropKnockDown
        # sensor and were explicitly rejected. No damage/push was applied.
        return (c['kind'] == 'r' and c['self'] == 'HipDropKnockDown' and c['self_type'] == '2'
                and c['answer'] == '0' and c['other'] in ('Attack','CheckPukupuku')
                and c['message'] in ('al::SensorMsgEnemyAttack','al::SensorMsgPush','rs::SensorMsgPushToPlayer','rs::SensorMsgIsExistPukupuku'))


def self_check():
    curve = Bezier([[0., 0., 0.], [1., 0., 0.], [2., 0., 0.], [3., 0., 0.]])
    # The original halves the endpoint contribution, including on a straight curve.
    assert curve.length == 2.94921875
    assert curve.parameter(0.) == 0. and curve.parameter(curve.length) == 1.


if __name__ == '__main__':
    self_check()
    print('Pukupuku rail checks passed.')
