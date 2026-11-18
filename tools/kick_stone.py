"""KickStone (obj1837) contact evidence: the stone's sensor traffic with Mario, judged against source.

The Forest stone is carried, thrown and left to roll while its Body sensor overlaps Mario's sensors.
The probe lists every overlap as contamination. This module proves what each overlap can do:

* Mario's receiveMsg (PlayerActorHakoniwa 0x428810) tests only the damage/bind/hack/jump/push families
  `carry_objects.mario_answer` lists. It has no test for KickStoneAttackHold, PlayerItemGet,
  BirdFlyAway or SeedItemGet (the aggregates it calls, rs::isMsgPlayerDamage and al::isMsgEnemyAttack,
  do not contain them), and messages that reach its attack sensors (Carry, HipDropKnockDown) are
  vetoed first. So everything the stone sends Mario is answered false, except CollidePush.
* The held stone's CollidePush (al::SensorMsgCollidePush, held collider feedback) is accepted by
  Mario's push receiver: the payload is the rewrite's own simulated held-collider feedback and must
  equal the recorded one bit for bit.
* KickStone::receiveMsg (0x2ab494) answers true to PlayerCarryFront through
  MapObjStatePlayerHold::tryStartCarryFront (0x2abb70, once, while waiting) and to PlayerRelease in the
  hold nerve (0x2abc98); the rest of the traffic from Mario's body sensors reaches no handler. The
  rewrite simulates both transitions, and the lifecycle it simulates (nerve and step from the pickup
  on, the keeper's phase/step/held state, the hold matrix) must equal the recorded one.
* Traffic from senders that are not Mario's body (the stone's own look-at queries, PaintObj and target-marker
  probes) is judged against the same dispatch (`stone_answer`): the messages no nerve tests are answered false,
  and the one that is tested, PlayerDisregard, is answered true exactly while the stone is held. A query whose
  answer depends on a nerve the frame does not pin down is refused rather than assumed.

Observations only validate the simulation, they never drive it. The stone's position is not claimed
here: the contacts' consequences (messages, answers, push payload, keeper and nerve lifecycle) are.
"""
import struct
from collections import defaultdict

from carry_objects import diagnostic, exact_vector, hold_matrix, mario_answer, words

STONE_ID = 'obj1837'
# Mario's own eye sensor only asks look-at questions (see environment_rule); everything else here is
# a physical sensor of Mario.
EYE = 'Eye'


# Message-name fragments of the families KickStone::receiveMsg tests: the attack list (0x2ab8b4-0x2aba18)
# in the nerves that run it (Wait, Fall, Roll, Water), the hold handlers (0x2b6ad8, 0x2b6c04, 0x2b6ddc)
# in Hold. Fall counts whole although the source reaches the attack list only from step 15.
ATTACK_NERVES = ('Wait', 'Fall', 'Roll', 'Water')
ATTACK_FAMILY = ('Kick', 'HipDrop', 'FireBall', 'CapAttack', 'EnemyAttack', 'Explosion', 'TRex', 'Tsukkun',
                 'Gamane', 'Gunetter', 'AmiiboKoopa', 'KuriboGirl', 'Packun', 'Sphinx', 'KillByHomeDemo')
HOLD_FAMILY = ('PlayerDisregard', 'Push', 'PlayerCarryKeepDemo', 'PlayerCarryShineGetStart', 'PlayerCarryShineGetEnd',
               'PlayerCarryCameraSubjectiveStart', 'PlayerCarryCameraSubjectiveEnd', 'PlayerCarryWarp',
               'HoldCancel', 'HoldRelease', 'PlayerReleaseDemo', 'ExplosionReflectBomb', 'PlayerCarryFront')


def gate(geometry):
    """The exported placement the Rust port accepts (movement/kick_stone.rs `sources`), or None."""
    if not geometry or (geometry.get('stage'), geometry.get('scenario')) != ('ForestWorldHomeStage', 1):
        return None
    for source in geometry.get('carry_objects', []):
        place = source.get('placement', {})
        if (source.get('id') == STONE_ID and place.get('Id') == STONE_ID and source.get('parent') == ''
                and place.get('UnitConfigName', place.get('UnitConfig', {}).get('ParameterConfigName')) == 'KickStone'
                and place.get('MoveType') == 0 and place.get('IsRandomKickCount') is False
                and place.get('KickCountMax') == 3 and place.get('IsConnectToCollision') is False
                and place.get('ItemType') == 'None'
                and place.get('Rotate') == dict(X=0., Y=0., Z=0.) and place.get('Scale') == dict(X=1., Y=1., Z=1.)
                and source.get('initial_phase') == 'init' and source.get('initial_alive') is True
                and source.get('initial_kick_count') == 3
                and source.get('sensors') == [dict(Name='Body', Radius=70., Type='EnemyBody')]
                and source.get('collider') == dict(Radius=60., Y=5.)):
            return source
    return None


def stone_ignores(message, nerve):
    """Whether KickStone::receiveMsg (0x2ab494) answers a Mario-sensor message false without acting on it.

    The attack/kick family is tested only while the stone waits, rolls or falls (0x2ab844-0x2ab8b0); the
    held stone answers its hold family (cancel, release, carry demos, disregard, push) and everything else
    falls through to false (0x2abe04). A message of a family the nerve handles is therefore never called
    inert here: it needs its own simulation.
    """
    if nerve in ATTACK_NERVES and any(token in message for token in ATTACK_FAMILY):
        return False
    return not (nerve == 'Hold' and message.split('SensorMsg')[-1] in HOLD_FAMILY)


# Messages the stone's non-Mario traffic carries that no nerve of KickStone::receiveMsg tests: they reach the
# fall-through at 0x2abe04 (only IgnoreIgnitionBomb is asked there), so every nerve answers them false.
UNTESTED = ('ConfirmPaintObj', 'PlayerLookAtPosition', 'PlayerDisregardTargetMarker', 'TargetMarkerPosition')


def stone_answer(message, holding):
    """The answer KickStone::receiveMsg (0x2ab494) gives to a message sent by something other than Mario.

    `holding` is whether the stone is in its Hold nerve when the message arrives (None: not known).
    PlayerDisregard is the one such message the dispatch answers: the Hold path calls
    MapObjStatePlayerHold::receiveMsgNoRelease (0x2b6ad8), whose first test (0x2b6b00) tail-calls al::isNerve on
    the hold state with the only nerve it has (set by its constructor 0x2b6894 and by appear 0x2b6a3c), so it is
    true while held; the other nerves never test it (0x2abe04: false). Returns None when the source does not
    settle the answer.
    """
    stem = message.split('SensorMsg')[-1]
    if stem in UNTESTED:
        return False
    if stem == 'PlayerDisregard' and holding is not None:
        return holding
    return None


def lifecycle(state):
    """The simulated nerve name and step (`Hold/20`), or None."""
    nerve, _, step = state.get('stone_nerve', '').partition('/')
    return (nerve, int(step)) if nerve and step.isdigit() else None


class Evidence:
    """Frames on which every message and answer of the stone's contacts is proved."""

    def __init__(self, geometry, objects, mario, messages, rewrite, rows, preroll):
        self.frames, self.pushing, self.identity, self.error = set(), set(), None, None
        self.stones = {STONE_ID}  # every KickStone of the stage; only STONE_ID has an observed nerve
        self.rejected = {}  # frame -> why the frame is not proved (for the judge's report)
        try:
            self.verify(geometry, objects, mario, messages, rewrite, rows)
        except (ValueError, KeyError, TypeError, IndexError, OverflowError, struct.error) as error:
            self.frames, self.pushing, self.identity = set(), set(), None
            self.error = str(error)

    def verify(self, geometry, objects, mario, messages, rewrite, rows):
        source = gate(geometry)
        if source is None or not objects or not mario or not messages or not rewrite or not rows:
            return
        initial = [o for o in objects if o['kind'] == 'init' and o['phase'] == 'init' and o['id'] == STONE_ID]
        self.stones |= {o['id'] for o in objects if o['class'] == 'KickStone'}
        place = source['placement']
        position = [place['Translate'][a] for a in 'XYZ']
        if len(initial) != 1 or initial[0]['dead'] != '0' or [float(x) for x in initial[0]['pos_values'].split('/')] != position:
            return
        samples = {int(o['frame']): o for o in objects
                   if o['kind'] == 'sample' and o['id'] == STONE_ID and o['phase'] == 'run'}
        keepers = {int(o['frame']): o for o in mario if o['kind'] == 'sample' and o['phase'] == 'run'}
        attachments = {int(o['frame']): o for o in mario if o['kind'] == 'attachment' and o['phase'] == 'run'}
        events = defaultdict(list)
        for event in messages:
            if event['phase'] == 'run':
                events[int(event['frame'])].append(event)
        payloads = {int(e['frame']): e for e in messages if e['phase'] == 'run' and e['kind'] == 'collide_payload'}

        observed = {f: (o['pending_n_name'] or o['current_n_name'], 0 if o['pending_n_name'] else int(o['step']))
                    for f, o in samples.items()}
        first_nerve = observed[min(observed)][0] if observed else None
        # Before the first transition the Wait step counts frames the actor has existed: that
        # depends on the stage's actor creation order, which neither the source nor this export shows.
        started = min((f for f, (n, _) in observed.items() if n != first_nerve), default=None)

        frames, pushing, identity = set(), set(), None
        for frame in sorted(rows):
            obj, keeper, state_row = samples.get(frame), keepers.get(frame), rewrite.get(frame)
            if obj is None or keeper is None or state_row is None:
                self.rejected[frame] = 'no observation or simulation row'
                continue
            token = obj['actor']
            if not token or token == '0' or keeper['dropped'] != '0' or obj['registry_dropped'] != '0':
                raise ValueError('missing object identity or dropped observations')
            current = (token, 'KickStone', source['host'])
            if identity is not None and identity != current:
                raise ValueError('object identity changed')
            identity = current
            if obj['dead'] != '0' or obj['clipped'] != '0' or obj['parent'] != '':
                self.rejected[frame] = 'actor clipped or dead (the port does not model that)'
                continue
            state = diagnostic(state_row)
            if not state.get('stone') or state.get('stone_limit'):
                self.rejected[frame] = 'stone simulation stopped: ' + state.get('stone_limit', 'no stone')
                continue
            simulated = lifecycle(state)
            nerve, step = observed[frame]
            if simulated is None or simulated[0] != nerve or (started is not None and frame >= started and simulated[1] != step):
                self.rejected[frame] = f'lifecycle {simulated} vs {(nerve, step)}'
                continue
            if not self.keeper_matches(state, keeper):
                self.rejected[frame] = 'keeper state differs'
                continue
            held = keeper['held_id'] == STONE_ID
            if frame in attachments:
                attachment = attachments[frame]
                previous = keepers.get(frame - 1, {})
                throw_hand = (previous.get('keeper_pending_n_name') or previous.get('keeper_current_n_name')) == 'Throw'
                if not exact_vector(words(hold_matrix(attachment, throw_hand)), attachment['hold_matrix_bits']):
                    raise ValueError('original attachment does not follow the hold-matrix equation')
            # The held collider runs inside the Hold nerve's execute (0x2ad014): on every frame whose current
            # nerve is Hold, including the one that requests the release, and on no other.
            pushes = obj['current_n_name'] == 'Hold'
            if pushes and not self.payload_matches(state, payloads.get(frame)):
                self.rejected[frame] = 'held collider feedback differs'
                continue
            unexplained = self.messages_match(frame, events[frame], state, observed, pushes, keeper)
            if unexplained:
                self.rejected[frame] = 'message or answer not explained by source: ' + unexplained
                continue
            frames.add(frame)
            if pushes:
                pushing.add(frame)
        self.frames, self.pushing, self.identity = frames, pushing, identity

    @staticmethod
    def keeper_matches(state, keeper):
        nerve = keeper['keeper_pending_n_name'] or keeper['keeper_current_n_name']
        if nerve not in ('Wait', 'Start', 'Carry', 'Throw'):
            return False
        held = keeper['held_id'] == STONE_ID
        phase = {'Wait': 0, 'Start': 1, 'Carry': 2, 'Throw': 3}[nerve]
        if state.get('keeper', '').split('/')[::2] != [str(phase), str(int(held))]:
            return False
        if phase and state.get('keeper') != f"{phase}/{keeper['keeper_step']}/{int(held)}":
            return False
        if held and (keeper['overhead'] != '0' or state.get('front_wall_keep') != keeper['wall_keep']
                     or 'held' not in state):
            return False
        return not (phase == 3 and held and state.get('inhibition') != keeper['inhibition'])

    @staticmethod
    def payload_matches(state, payload):
        """The held collider's feedback the stone sends Mario, against the recorded payload."""
        return (payload is not None and payload['message_type'] == 'al::SensorMsgCollidePush'
                and exact_vector(state.get('stone_push', ''), payload['payload_bits']))

    def messages_match(self, frame, events, state, observed, pushes, keeper):
        """Every message between Mario's physical sensors and the stone, with the answer the source gives it."""
        previous = observed.get(frame - 1, (None, 0))[0]
        nerve = observed[frame][0]
        picked_up = nerve == 'Hold' and previous != 'Hold'
        released = previous == 'Hold' and nerve != 'Hold'
        for event in events:
            if event['kind'] not in ('object_receive_after', 'mario_receive_after', 'mario_push_after'):
                continue  # before-events carry no answer; send/push-translation rows carry no message
            to_stone = event['kind'] == 'object_receive_after'
            if to_stone and event['sender_id'] != 'Mario':
                # PaintObj confirmations, target-marker queries and the stone's own look-at traffic come from
                # sensors that are not Mario's: no Mario contact, but the stone still answers them (0x2ab494).
                unexplained = self.query_mismatch(event, frame, events, observed)
                if unexplained:
                    return unexplained
                continue
            mario_sensor = event['sender_sensor'] if to_stone else event['receiver_sensor']
            if mario_sensor == EYE:
                continue
            message = event['message_type']
            if to_stone:
                if event['receiver_id'] != STONE_ID:
                    return f'{message} reaches {event["receiver_id"]}'
                if message == 'al::SensorMsgPlayerCarryFront':
                    expected = picked_up and event['sender_sensor'] == 'Body'
                elif message == 'al::SensorMsgPlayerRelease':
                    expected = (released and state.get('keeper') == '3/6/0'
                                and event['keeper_current_n_name'] == 'Throw' and event['keeper_step'] == '5')
                elif not stone_ignores(message, nerve):
                    return f'{message} is handled in {nerve}'
                else:
                    expected = False
            else:
                if event['sender_id'] != STONE_ID or event['receiver_id'] != 'Mario':
                    return f'{message} from {event["sender_id"]}'
                if message == 'al::SensorMsgCollidePush':
                    expected = pushes and event['sender_sensor'] == 'Body' and event['receiver_sensor'] == 'Body'
                else:
                    expected = mario_answer(message, event['receiver_sensor'])
                    if expected is None:
                        return f'{message} has no source answer'
            if event['answer'] != str(int(expected)):
                return f"{message} {mario_sensor} answered {event['answer']}, expected {int(expected)}"
        return None

    def query_mismatch(self, event, frame, events, observed):
        """Why a message to a stone from a sender other than Mario's body has an unproved answer, or None."""
        message, receiver = event['message_type'], event['receiver_id']
        if receiver not in self.stones:
            return f'{message} reaches {receiver}'
        holding = self.holding_at(event, frame, events, observed) if receiver == STONE_ID else None
        expected = stone_answer(message, holding)
        if expected is None:
            return f'{message} to {receiver}: the source does not settle its answer here'
        if event['answer'] != str(int(expected)):
            return f"{message} to {receiver} answered {event['answer']}, expected {int(expected)}"
        return None

    @staticmethod
    def holding_at(event, frame, events, observed):
        """Whether the stone is held when `event` arrives (None when the frame does not pin it down).

        A frame's observation is taken after its messages, so the nerve a message meets is the previous frame's
        observation. When the frame changes it, the only change into or out of Hold is Mario's PlayerCarryFront
        or PlayerRelease answered true: messages ordered before it meet the old nerve, later ones the new one.
        """
        end = observed[frame][0] == 'Hold'
        start = end if frame - 1 not in observed else observed[frame - 1][0] == 'Hold'
        if start == end:
            return end
        for other in events:
            if (other['kind'] == 'object_receive_after' and other['sender_id'] == 'Mario' and other['answer'] == '1'
                    and other['message_type'] in ('al::SensorMsgPlayerCarryFront', 'al::SensorMsgPlayerRelease')):
                return end if int(other['order']) < int(event['order']) else start
        return None

    def contact(self, frame, contact):
        """Whether one typed contact row of Mario with the stone is covered by the proved frame."""
        if frame not in self.frames or not self.identity or contact['who'] != 'player':
            return False
        if (contact['actor'], contact['host_class'], contact['host']) != self.identity:
            return False
        if contact['kind'] == 'a':
            return True
        if contact['kind'] not in ('r', 'p'):
            return False
        if contact['message'] == 'al::SensorMsgCollidePush':
            return (frame in self.pushing and contact['answer'] == '1'
                    and (contact['self'], contact['other']) == ('Body', 'Body'))
        answer = mario_answer(contact['message'], contact['self'])
        return answer is not None and contact['answer'] == str(int(answer))


def self_check():
    assert gate(None) is None and gate({'stage': 'ForestWorldHomeStage', 'scenario': 1}) is None
    assert not Evidence(None, None, None, None, None, None, None).frames
    state = {'keeper': '3/6/0'}
    assert Evidence.keeper_matches({'keeper': '0/0/0'}, dict(keeper_pending_n_name='', keeper_current_n_name='Wait',
                                                              held_id='none', keeper_step='5', overhead='0', wall_keep='0',
                                                              inhibition='0')) is True
    assert not Evidence.keeper_matches(state, dict(keeper_pending_n_name='Wait', keeper_current_n_name='',
                                                    held_id='none', keeper_step='0', overhead='0', wall_keep='0',
                                                    inhibition='0'))
    assert Evidence.payload_matches({'stone_push': '00000000/00000000/00000000'},
                                    dict(message_type='al::SensorMsgCollidePush', payload_bits='00000000,00000000,00000000'))
    assert not Evidence.payload_matches({'stone_push': '00000000/00000000/3f800000'},
                                        dict(message_type='al::SensorMsgCollidePush', payload_bits='00000000,00000000,00000000'))
    assert not Evidence.payload_matches({'stone_push': '00000000/00000000/00000000'}, None)
    assert lifecycle({'stone_nerve': 'Hold/20'}) == ('Hold', 20) and lifecycle({}) is None
    # The kick family acts on a waiting, falling or rolling stone, never on a held one; the hold family is the reverse.
    assert stone_ignores('al::SensorMsgPlayerAttackKick', 'Hold')
    assert not stone_ignores('al::SensorMsgPlayerAttackKick', 'Wait')
    assert not stone_ignores('rs::SensorMsgPlayerHipDropAttack', 'Roll')
    assert not stone_ignores('al::SensorMsgHoldCancel', 'Hold') and stone_ignores('al::SensorMsgHoldCancel', 'Fall')
    assert stone_ignores('rs::SensorMsgPlayerCapPush', 'Hold') and stone_ignores('al::SensorMsgPlayerItemGet', 'Fall')
    # Message judging on synthetic frames: an inert kick on a held stone passes, the same kick on a waiting
    # stone (the source handles it there), a Mario answer the source does not give, or a stranger's
    # message do not.
    def event(kind, message, sender, sensor, receiver, answer, receiver_sensor='Body', order=0):
        return dict(kind=kind, message_type=message, sender_id=sender, sender_sensor=sensor, receiver_id=receiver,
                    receiver_sensor=receiver_sensor, answer=answer, keeper_current_n_name='Wait', keeper_step='0',
                    order=str(order))
    judge = Evidence(None, None, None, None, None, None, None)
    judge.stones.add('obj1838')
    kick = event('object_receive_after', 'al::SensorMsgPlayerAttackKick', 'Mario', 'Foot', STONE_ID, '0')
    assert judge.messages_match(5, [kick], {}, {5: ('Hold', 3)}, True, {}) is None
    assert judge.messages_match(5, [kick], {}, {5: ('Wait', 3)}, False, {})
    stranger = event('object_receive_after', 'rs::SensorMsgConfirmPaintObj', 'unknown', 'Body', STONE_ID, '0')
    assert judge.messages_match(5, [stranger], {}, {5: ('Wait', 3)}, False, {}) is None
    stranger['answer'] = '1'
    assert judge.messages_match(5, [stranger], {}, {5: ('Wait', 3)}, False, {})
    # The stone's own PlayerDisregard query: true while held, false otherwise, and in the pickup frame it depends
    # on whether it came before the CarryFront that starts the hold. Another stone's nerve is unknown: refused.
    own = event('object_receive_after', 'al::SensorMsgPlayerDisregard', STONE_ID, 'Body', STONE_ID, '1', order=9)
    assert judge.messages_match(5, [own], {}, {4: ('Hold', 3), 5: ('Hold', 4)}, True, {}) is None
    own['answer'] = '0'
    assert judge.messages_match(5, [own], {}, {4: ('Hold', 3), 5: ('Hold', 4)}, True, {})
    assert judge.messages_match(5, [own], {}, {4: ('Wait', 3), 5: ('Wait', 4)}, False, {}) is None
    own['answer'] = '1'
    assert judge.messages_match(5, [own], {}, {4: ('Wait', 3), 5: ('Wait', 4)}, False, {})
    pickup = event('object_receive_after', 'al::SensorMsgPlayerCarryFront', 'Mario', 'Body', STONE_ID, '1', order=5)
    states = {4: ('Wait', 3), 5: ('Hold', 0)}
    before, after = dict(own, order='3', answer='0'), dict(own, order='7', answer='1')
    assert judge.messages_match(5, [before, pickup, after], {}, states, False, {}) is None
    assert judge.messages_match(5, [dict(before, answer='1'), pickup], {}, states, False, {})
    assert judge.messages_match(5, [pickup, dict(after, answer='0')], {}, states, False, {})
    assert judge.messages_match(5, [before], {}, states, False, {})  # nerve changed by something not in the frame
    other = event('object_receive_after', 'al::SensorMsgPlayerDisregard', STONE_ID, 'Body', 'obj1838', '0')
    assert judge.messages_match(5, [other], {}, {5: ('Wait', 3)}, False, {})
    paint = event('object_receive_after', 'rs::SensorMsgConfirmPaintObj', 'unknown', 'Body', 'obj1838', '0')
    assert judge.messages_match(5, [paint], {}, {5: ('Wait', 3)}, False, {}) is None
    paint['receiver_id'] = 'obj9999'
    assert judge.messages_match(5, [paint], {}, {5: ('Wait', 3)}, False, {})
    mystery = event('object_receive_after', 'rs::SensorMsgNoSuchQuery', 'unknown', 'Body', STONE_ID, '0')
    assert judge.messages_match(5, [mystery], {}, {5: ('Wait', 3)}, False, {})
    hit = event('mario_receive_after', 'al::SensorMsgPlayerItemGet', STONE_ID, 'Body', 'Mario', '0', 'Head')
    assert judge.messages_match(5, [hit], {}, {5: ('Fall', 3)}, False, {}) is None
    hit['answer'] = '1'
    assert judge.messages_match(5, [hit], {}, {5: ('Fall', 3)}, False, {})
    unknown = event('mario_receive_after', 'rs::SensorMsgHit', STONE_ID, 'Body', 'Mario', '0')
    assert judge.messages_match(5, [unknown], {}, {5: ('Fall', 3)}, False, {})



if __name__ == '__main__':
    self_check()
