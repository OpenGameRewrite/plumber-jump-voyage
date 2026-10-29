"""Placement-bound CoinChameleon overlap evidence. Any other actor stays refused.

A transparent coin reveals itself when it accepts an `ItemGetAll` on its `Body` sensor or a hip
drop near its `Hipdrop` sensor (docs/fidelity-harness.md, environment rule). The probe's
ceiling-check side file (`game.ceiling_check.csv`, FIDCEIL10) records every message the player
sent to a coin, which sensor pair carried it and what the coin answered. An overlap with a coin
is accepted only when that record proves the coin accepted exactly one kind of message,
`SensorMsgPlayerDisregardTargetMarker` from Mario's `Eye` to the coin's `Body` (a look-at query),
with full buffer coverage, and when the coin's recorded origin is a placement exported from the
stage (translation plus the UnitConfig display offset). Nothing here exempts coins in general.
"""
import struct

# vtable of `al::SensorMsgPlayerDisregardTargetMarker`, main-relative; relocation-derived in the
# wave-10 ceiling recordings (docs/fidelity-harness.md).
DISREGARD_TARGET_MARKER = '1d9d8a8'


def f32(value):
    return struct.unpack('<f', struct.pack('<f', value))[0]


def bits(value):
    return struct.pack('>f', value).hex()


def origin_bits(placement):
    """The exact bits of a coin's runtime origin as the probe writes them."""
    return ','.join(bits(f32(f32(t) + f32(o))) for t, o in zip(placement['translate'], placement['display_offset']))


class Evidence:
    def __init__(self, geometry, side, rows):
        self.tokens = {}  # probe actor id -> exported placement id
        try:
            self.verify(geometry, side, rows)
        except (ValueError, KeyError, TypeError, IndexError):
            self.tokens = {}

    def verify(self, geometry, side, rows):
        if not geometry or not side or not rows:
            return
        placements = {}
        for placement in geometry.get('coin_chameleons', []):
            if placement.get('links') or any(placement['rotate']):
                continue
            placements[origin_bits(placement)] = placement['id']
        samples = [r for r in side if r['kind'] == 'sample']
        # The record only proves anything when no observation was lost.
        if not samples or any(r['dropped'] != '0' for r in samples):
            raise ValueError('incomplete coin message record')
        if not set(rows) <= {int(r['frame']) for r in samples}:
            raise ValueError('coin message record does not cover the run')
        positions = {}
        for r in side:
            if r['kind'] != 'coin_receive':
                continue
            if r['result'] not in ('0', '1'):
                raise ValueError('unresolved coin answer')
            if r['result'] == '1' and (r['msg_vt'], r['coin_sensor'], r['player_sensor']) != (
                    DISREGARD_TARGET_MARKER, 'Body', 'Eye'):
                raise ValueError('the coin accepted a message other than the look-at query')
            positions.setdefault(r['token'], set()).add(r['coin_pos_bits'])
        tokens = {}
        for token, seen in positions.items():
            if len(seen) != 1 or next(iter(seen)) not in placements:
                raise ValueError('coin origin is not an exported placement')
            tokens[token] = placements[next(iter(seen))]
        if len(set(tokens.values())) != len(tokens):
            raise ValueError('two probe actors share one placement')
        self.tokens = tokens

    def contact(self, frame, c):
        """Whether a typed contact is an overlap with one of the proven coins, nothing sent either way."""
        return (c['who'] == 'player' and c['kind'] == 'a' and c['host_class'] == 'CoinChameleon'
                and c['actor'] in self.tokens and not c['message'] and not c['answer']
                and c['other'] in ('Hipdrop', 'Body'))


def self_check():
    placement = dict(id='obj1', translate=[1., 2., 3.], display_offset=[0., 75., 0.], rotate=[0., 0., 0.], links=False)
    geometry = dict(coin_chameleons=[placement])
    pos = origin_bits(placement)

    def side(result='0', vt='1d9d8a8', sensor='Body', player='Eye', dropped='0', position=pos):
        return [dict(kind='sample', frame='0', dropped=dropped),
                dict(kind='coin_receive', token='1', result=result, msg_vt=vt, coin_sensor=sensor,
                     player_sensor=player, coin_pos_bits=position)]

    contact = dict(who='player', kind='a', host_class='CoinChameleon', actor='1', message='', answer='',
                   other='Hipdrop')
    rows = {0: {}}
    assert Evidence(geometry, side('1'), rows).contact(0, contact)
    assert not Evidence(geometry, side('1', vt='1d9d718'), rows).contact(0, contact)
    assert not Evidence(geometry, side('1', sensor='Hipdrop'), rows).contact(0, contact)
    assert not Evidence(geometry, side(dropped='1'), rows).contact(0, contact)
    assert not Evidence(geometry, side(position='00000000,00000000,00000000'), rows).contact(0, contact)
    assert not Evidence(geometry, side('1'), {5: {}}).contact(0, contact)
    assert not Evidence(geometry, side('1'), rows).contact(0, dict(contact, host_class='Coin'))
    assert not Evidence(geometry, side('1'), rows).contact(0, dict(contact, actor='2'))
    assert not Evidence(None, side('1'), rows).contact(0, contact)
