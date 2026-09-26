#!/usr/bin/env python3
"""Pack linter tests: valid pack, each failure mode, and runtime loading."""
import copy, json
import sl_parser as sp, sl_pack as pk

good = json.load(open('example_pack.json', encoding='utf8'))
results = []
def t(name, cond):
    print(('PASS ' if cond else 'FAIL ') + name); results.append(cond)

def codes(m, others=()): return {p['code'] for p in pk.lint_pack(m, others)}
def variant(**kw):
    m = copy.deepcopy(good); m.update(kw); return m

t('example pack is valid', pk.lint_pack(good) == [])
t('hash is stable and ignores the hash field', pk.pack_hash(good) == pk.pack_hash({**good, 'hash': 'x'}))
t('hash changes when content changes', pk.pack_hash(good) != pk.pack_hash(variant(version='1.0.1')))
t('word colliding with core is rejected', 'P_COLLISION' in codes(variant(words=[{'lemma': 'hostis', 'pos': 'noun', 'gloss': 'enemy'}])))
t('verb colliding with core forms is rejected', 'P_COLLISION' in codes(variant(verbs=[{'stem': 'scan', 'gloss': 'scan', 'valency': 'trans'}])))
t('word equal to a generated verb form is rejected', 'P_COLLISION' in codes(variant(words=[{'lemma': 'scanatum', 'pos': 'adj', 'gloss': 'x'}])))
t('closed-class word is rejected', 'P_POS' in codes(variant(words=[{'lemma': 'apud', 'pos': 'prep', 'gloss': 'at'}])))
t('missing gloss is rejected', 'P_GLOSS' in codes(variant(words=[{'lemma': 'noctem', 'pos': 'noun', 'gloss': ''}])))
t('non-ASCII lemma is rejected', 'P_NAME' in codes(variant(words=[{'lemma': 'umbr\u00e4', 'pos': 'noun', 'gloss': 'x'}])))
t('duplicate word inside a pack is rejected', 'P_COLLISION' in codes(variant(words=[{'lemma': 'noctem', 'pos': 'noun', 'gloss': 'x'}, {'lemma': 'noctem', 'pos': 'noun', 'gloss': 'y'}])))
t('wrong spec major is rejected', 'P_SPEC' in codes(variant(spec='2.0')))
t('invalid phrase template is rejected', 'P_TEMPLATE' in codes(variant(phrases=[{'template': 'Scana scana scana.', 'tags': ['x']}, ])) or 'P_TEMPLATE' in codes(variant(phrases=[{'template': 'Hostis {noun} {noun} {noun} detectum.', 'tags': ['x']}])))
t('unknown slot is rejected', 'P_TEMPLATE' in codes(variant(phrases=[{'template': 'Scana {thing}.', 'tags': ['x']}])))
t('collision with another pack is rejected', 'P_COLLISION' in codes(variant(pack='other'), others=[good]))
t('too many words is rejected', 'P_LIMIT' in codes(variant(words=[{'lemma': f'wa{chr(97 + i // 26)}{chr(97 + i % 26)}', 'pos': 'noun', 'gloss': 'x'} for i in range(501)])))

snap = sp.snapshot()
pk.apply_pack(good)
ok1 = sp.check('Salutamus umbra.')[0] and sp.check('Zona obscurum.')[0] and sp.check('Salta.')[0]
bad1 = not sp.check('Salta nodus.')[0]                        # intransitive pack verb takes no object
sp.restore(snap)
t('pack words parse once the pack is loaded', ok1)
t('pack verb valency is enforced', bad1)
t('restore removes the pack again', not sp.check('Zona obscurum.')[0])
print(sum(results), '/', len(results), 'pack tests passed')
raise SystemExit(0 if all(results) else 1)
