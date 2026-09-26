#!/usr/bin/env python3
"""Vocabulary pack tools (spec v1.2): lint, hash, and load packs.

    python3 sl_pack.py lint example_pack.json
    python3 sl_pack.py hash example_pack.json
"""
import hashlib, itertools, json, re, sys
import sl_parser as sp

SPEC_VERSION = '1.2'
PACK_POS = {'noun', 'adj', 'adv'}                  # closed classes and grammar words are core-only
LIMITS = {'verbs': 50, 'words': 500, 'phrases': 200}
SLOTS = {'noun': None, 'adj': None, 'number': ['tria', 'duo decem'], 'code': ['codex duo quattuor septem']}

def canonical_json(manifest):
    body = {k: v for k, v in manifest.items() if k not in ('hash', 'signature')}
    return json.dumps(body, sort_keys=True, separators=(',', ':'), ensure_ascii=True)

def pack_hash(manifest):
    return 'sha256:' + hashlib.sha256(canonical_json(manifest).encode('ascii')).hexdigest()

def err(code, msg): return {'code': code, 'msg': msg}

def _word_ok(w): return isinstance(w, str) and re.fullmatch(r'[a-z]{2,24}', w) is not None

def lint_pack(m, others=()):
    """Return a list of problems (empty list = pack is valid)."""
    problems = []
    for key in ('pack', 'version', 'spec', 'license', 'author'):
        if not isinstance(m.get(key), str) or not m[key]:
            problems.append(err('P_SCHEMA', f"missing or empty '{key}'"))
    if problems: return problems
    if not re.fullmatch(r'[a-z][a-z0-9-]{1,39}', m['pack']): problems.append(err('P_NAME', 'pack name must be a lowercase slug'))
    if not re.fullmatch(r'\d+\.\d+\.\d+', m['version']): problems.append(err('P_SCHEMA', 'version must be semver x.y.z'))
    if m['spec'].split('.')[0] != SPEC_VERSION.split('.')[0]: problems.append(err('P_SPEC', f"pack targets spec {m['spec']}, this toolkit is {SPEC_VERSION}"))
    verbs, words, phrases = m.get('verbs', []), m.get('words', []), m.get('phrases', [])
    for name, items in (('verbs', verbs), ('words', words), ('phrases', phrases)):
        if len(items) > LIMITS[name]: problems.append(err('P_LIMIT', f'more than {LIMITS[name]} {name}'))
    if not (verbs or words or phrases): problems.append(err('P_SCHEMA', 'pack is empty'))

    taken = {}                                        # surface -> owner, across core and other packs
    for surface in sp.INDEX: taken[surface] = 'core'
    for o in others:
        for v in o.get('verbs', []):
            for surf, _ in sp._verb_analyses(v['stem']): taken.setdefault(surf, o['pack'])
        for w in o.get('words', []): taken.setdefault(w['lemma'], o['pack'])

    mine = set()
    def claim(surface, what):
        if surface in taken: problems.append(err('P_COLLISION', f"{what} '{surface}' already exists in {taken[surface]}"))
        elif surface in mine: problems.append(err('P_COLLISION', f"{what} '{surface}' is defined twice in this pack"))
        mine.add(surface)

    for v in verbs:
        stem = v.get('stem')
        if not _word_ok(stem): problems.append(err('P_NAME', f"bad verb stem {stem!r}")); continue
        if not v.get('gloss'): problems.append(err('P_GLOSS', f"verb '{stem}' needs a gloss"))
        if v.get('valency') not in ('trans', 'intrans'): problems.append(err('P_SCHEMA', f"verb '{stem}' needs valency trans or intrans"))
        for surf, _ in sp._verb_analyses(stem): claim(surf, f"form of '{stem}a'")
    for w in words:
        lemma = w.get('lemma')
        if not _word_ok(lemma): problems.append(err('P_NAME', f"bad lemma {lemma!r}")); continue
        if not w.get('gloss'): problems.append(err('P_GLOSS', f"word '{lemma}' needs a gloss"))
        if w.get('pos') not in PACK_POS: problems.append(err('P_POS', f"word '{lemma}': packs may add only {sorted(PACK_POS)}")); continue
        claim(lemma, 'word')

    if problems: return problems
    # phrase templates: apply the pack, fill every slot with samples, and require every fill to parse
    snap = sp.snapshot()
    try:
        apply_pack(m)
        nouns = sorted(k for k, e in sp.WORDS.items() if 'noun' in e['pos'] and k != 'codex')
        adjs = sorted(k for k, e in sp.WORDS.items() if 'adj' in e['pos'])
        packn = [w['lemma'] for w in words if w['pos'] == 'noun']; packa = [w['lemma'] for w in words if w['pos'] == 'adj']
        pools = {'noun': nouns[:4] + packn, 'adj': adjs[:4] + packa, 'number': SLOTS['number'], 'code': SLOTS['code']}
        for ph in phrases:
            t = ph.get('template', '')
            if not isinstance(ph.get('tags'), list) or not ph['tags']: problems.append(err('P_SCHEMA', f'phrase needs tags: {t!r}'))
            if not isinstance(ph.get('weight', 1), int) or not 1 <= ph.get('weight', 1) <= 10: problems.append(err('P_SCHEMA', f'weight must be 1-10: {t!r}'))
            slots = re.findall(r'\{(\w+)\}', t)
            if any(x not in pools for x in slots): problems.append(err('P_TEMPLATE', f'unknown slot in {t!r}')); continue
            combos = list(itertools.product(*[pools[x] for x in slots]))[:80]
            for combo in combos or [()]:
                text = t
                for x, val in zip(slots, combo): text = text.replace('{' + x + '}', val, 1)
                ok, res = sp.check(text)
                if not ok:
                    problems.append(err('P_TEMPLATE', f'{t!r} fails as {text!r}: {res["code"]}')); break
    finally:
        sp.restore(snap)
    return problems

def apply_pack(m):
    """Add a (linted) pack's verbs and words to the running parser."""
    for v in m.get('verbs', []):
        sp.add_verb(v['stem'], {'gloss': v['gloss'], 'valency': v['valency'], 'ct': v['stem'].endswith('ct')})
    for w in m.get('words', []):
        sp.add_word(w['lemma'], {'pos': [w['pos']], 'gloss': w['gloss']})

if __name__ == '__main__':
    if len(sys.argv) < 3 or sys.argv[1] not in ('lint', 'hash'):
        print(__doc__); sys.exit(2)
    m = json.load(open(sys.argv[2], encoding='utf8'))
    if sys.argv[1] == 'hash':
        print(pack_hash(m)); sys.exit(0)
    probs = lint_pack(m)
    if not probs:
        print(f"OK  {m['pack']} {m['version']}  {pack_hash(m)}")
    for p in probs: print(f"{p['code']}: {p['msg']}")
    sys.exit(1 if probs else 0)
