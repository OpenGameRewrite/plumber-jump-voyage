"""English names for the original's hit-reaction events.

The game's sound and effect tables key their event lists by Japanese
reaction names. The sound and effect tools translate the ones the rewrite
raises with this one table, and the game looks them up by the same English
names (see src/reactions.rs).
"""
MARIO_REACTIONS = {
    '着地': 'Land',
    '着地[走り]': 'LandRun',
    '転がり再加速[1回目]': 'RollBoost',
    '転がり再加速[2回目以降]': 'RollBoostAgain',
    '転がり着地': 'RollLand',
    'あちちジャンプ': 'BurnJump',
    'あちちジャンプ(バウンド)': 'BurnBounce',
    'アチチ走り浅瀬キャンセル': 'FireQuench',
}
CAP_REACTIONS = {'帽子投げ': 'Throw', '帽子戻し': 'Return', '帽子キャッチ': 'Catch'}
# Reactions only the effect tables listen to.
EFFECT_REACTIONS = {
    'ヒップドロップ着地': 'HipDropLand',
    'ヒップドロップ着地[弱]': 'HipDropLandWeak',
    '高所着地': 'LandFall',
    'アクションジャンプ': 'ActionJump',
    '汚れ落とし(スス)': 'SootWash',
    '入水落下': 'WaterDive',
    '帽子入水': 'CapEnterWater',
    '帽子出水': 'CapLeaveWater',
    '踏みつけヒット': 'StompHit',
    'ヒップドロップヒット': 'PoundHit',
    '帽子ヒット': 'CapHit',
    '帽子反射ヒット': 'CapBounceHit',
}


def english(reaction):
    """The English name of a reaction, or the original name when none is used."""
    for table in (MARIO_REACTIONS, CAP_REACTIONS, EFFECT_REACTIONS):
        if reaction in table:
            return table[reaction]
    return reaction


def self_check():
    assert english('着地') == 'Land' and english('帽子キャッチ') == 'Catch'
    assert english('高所着地') == 'LandFall'
    assert english('unlisted') == 'unlisted'
    print('Reaction name checks passed')
