#!/usr/bin/env python3
"""Build sl_lexicon.json from the System Latin spec (markdown)."""
import re, json, sys, unicodedata

SPEC = sys.argv[1] if len(sys.argv) > 1 else '../system_latin_systemized.md'
OUT = sys.argv[2] if len(sys.argv) > 2 else 'sl_lexicon.json'

def canon(w):
    w = unicodedata.normalize('NFKD', w.lower())
    return ''.join(c for c in w if not unicodedata.combining(c))

t = open(SPEC, encoding='utf8').read()
main = t.split('# XLII. GRAMMAR')[0]

# ---------------- verbs ----------------
sec = t.split('# XLIV. VERB PARADIGMS')[1].split('### Full conjugation')[0]
cmds = re.findall(r'^\| \*\*([A-Za-z]+)\.\*\* \|', sec, re.M)
stems = [c[:-1].lower() for c in cmds]
gloss_cmd = {}
sec1 = main.split('# I. CORE COMMANDS')[1].split('# II. ACKNOWLEDGMENT')[0]
for m in re.finditer(r'^\| \*\*([A-Za-z]+)\.\*\* \| ([^|]+?) \|', sec1, re.M):
    gloss_cmd[m.group(1).lower()] = m.group(2).strip().rstrip('.').lower()
manual_gloss = {'sent': 'sense', 'requir': 'require', 'immin': 'be imminent', 'appropinqu': 'approach',
                'tent': 'try', 'pon': 'set', 'leg': 'read', 'urg': 'be urgent', 'suffici': 'suffice',
                'const': 'remain constant', 'arm': 'arm', 'oti': 'idle', 'accip': 'accept', 'zet': 'search for'}
INTRANS = {'st', 'man', 'progred', 'redi', 'exi', 'appropinqu', 'reced', 'evad', 'const', 'immin',
           'exspect', 'paus', 'urg', 'suffici', 'iter', 'oti', 'vol'}
verbs = {}
for s in stems:
    g = manual_gloss.get(s) or gloss_cmd.get(s + 'a') or s
    verbs[s] = {'gloss': g, 'valency': 'intrans' if s in INTRANS else 'trans', 'ct': s.endswith('ct')}

# ---------------- generated forms (to exclude from word list) ----------------
def forms(s):
    ct = s.endswith('ct'); f = {s + 'a', s + 'are', s + 'ans', s + 'andum'}
    for e in ['o', 'as', 'at', 'amus', 'atis', 'ant']:
        f |= {s + e, s + 'av' + e, s + 'abs' + e}
    f |= {s + 'atur', s + 'antur', s + 'absatur', s + 'absantur',
          s + ('um' if ct else 'atum'), s + ('io' if ct else 'atio'), s + ('or' if ct else 'ator')}
    return f
gen = set().union(*[forms(s) for s in stems])

# ---------------- closed classes ----------------
PREP = {'ad', 'ab', 'in', 'ex', 'sub', 'super', 'inter', 'intus', 'extra', 'circa', 'contra', 'trans', 'per', 'sine',
        'ante', 'post', 'infra', 'supra', 'prope', 'longe', 'retro'}
PREP_ADV = {'ante', 'post', 'infra', 'supra', 'prope', 'longe', 'retro'}
ADV = {'nunc', 'statim', 'semper', 'numquam', 'iam', 'postea', 'iterum'}  # closed-class temporal/frequency particles; manner adverbs are derived from adjectives (see sl_parser.add_word)
RETIRED = {'celeriter', 'cito', 'tarde', 'tarditer'}  # v2 systemization: these surface forms are derived at runtime from celerum/tardum, not listed statically
PRON = {'ego', 'tu', 'id', 'nos', 'vos', 'ea'}
QUANT = {'omnia', 'multum', 'nullum', 'aliquid'}
QW = {'utrum', 'quis', 'quid', 'quale', 'ubi', 'cur', 'quomodo', 'quantum', 'quando'}
RULE = {'si', 'nisi', 'tunc', 'aliter', 'quando', 'dum', 'donec', 'quisque'}
CONJ = {'et', 'aut'}
NEG = {'non'}
ANS = {'affirmatum', 'negativum'}
CMP = {'aequale', 'maius', 'minus'}
DIGITS = ['zero', 'unum', 'duo', 'tria', 'quattuor', 'quinque', 'sex', 'septem', 'octo', 'novem']
PLACES = ['decem', 'centum', 'mille', 'milio']
NUMOP = {'punctum', 'negatum', 'ordo'}
DEM = {'hic': 'this (near)', 'ille': 'that (far)', 'idem': 'the same', 'alius': 'other'}
ART = {'to': 'the', 'ta': 'the (plural)', 'ti': 'a / an (indefinite)'}
ADJ = set('''invalidum incognitum imparatum validum incorrigatum possibilum impossibilum necessarium optionalum
activum inactivum completum incompletum occupatum disponibilum indisponibilum stabilum instabilum normalum criticum
novum vetusum primarium secundarium tertiarium ultimum proximum prioritarium inermum liberum periculosum securum
sinistrum dextrum rectum violatum incertum privatum publicum plenum maximum minimum altum humilum debilum fortum
probabilum improbabilum certum conditionalum verum falsum immediatum remotum novissimum autonomum manualum
automaticum tutum hostilum primum priusum insufficians criticum celerum tardum chaosum'''.split())

ADJ.add('minus')  # also a comparator (see CMP)

# ---------------- gloss table for non-verb words ----------------
body = main.split('# II. ACKNOWLEDGMENT')[1]
glosses = {}
for m in re.finditer(r'^\| \*\*([^*|]+?)\*\* \| ([^|]+?) \|', body, re.M):
    h = canon(m.group(1)).strip().rstrip('.')
    if ' ' not in h:
        glosses.setdefault(h, canon(m.group(2)).strip().rstrip('.'))
words = {}
def add(w, pos, gloss=None):
    e = words.setdefault(w, {'pos': [], 'gloss': gloss or glosses.get(w, '')})
    if pos not in e['pos']:
        e['pos'].append(pos)
    if gloss and not e['gloss']:
        e['gloss'] = gloss

toks = []
for m in re.finditer(r'^\| \*\*(.+?)\*\* \|', body, re.M):
    for w in re.findall(r"[a-zōīāēū]+", m.group(1).lower()):
        toks.append(canon(w))
for w in dict.fromkeys(toks):
    if w in gen or w in {'online', 'offline'} or w in RETIRED:
        continue
    if w in PREP: add(w, 'prep')
    if w in PREP_ADV or w in ADV: add(w, 'adv')
    if w in PRON: add(w, 'pron')
    if w in QUANT: add(w, 'quant')
    if w in ADJ: add(w, 'adj')
    if w in DIGITS or w in PLACES: continue
    if not (w in PREP or w in ADV or w in PRON or w in QUANT or w in ADJ or w in RULE or w in CONJ or w in NEG or w in QW
            or w in CMP or w in ANS or w in NUMOP):
        add(w, 'noun')
# additions from later sections / closed classes
for w in PREP: add(w, 'prep')
for w in PREP_ADV | ADV: add(w, 'adv')
for w in PRON: add(w, 'pron')
for w, g in {'omnia': 'all', 'multum': 'many / much', 'nullum': 'none / no', 'aliquid': 'any'}.items(): add(w, 'quant', g)
for w in ADJ: add(w, 'adj')
for w in CMP: add(w, 'cmp', {'aequale': 'equal', 'maius': 'greater', 'minus': 'less'}[w])
for w, g in {'si': 'if', 'nisi': 'unless', 'tunc': 'then', 'aliter': 'else', 'quando': 'when', 'dum': 'while',
             'donec': 'until', 'quisque': 'each'}.items(): add(w, 'rule', g)
for w, g in {'et': 'and', 'aut': 'or'}.items(): add(w, 'conj', g)
add('non', 'neg', 'not')
for w, g in {'utrum': 'whether', 'quis': 'who', 'quid': 'what', 'quale': 'which', 'ubi': 'where', 'cur': 'why',
             'quomodo': 'how', 'quantum': 'how much / many', 'quando': 'when'}.items(): add(w, 'qw', g)
for w, g in {'affirmatum': 'yes', 'negativum': 'no'}.items(): add(w, 'ans', g)
for i, w in enumerate(DIGITS): add(w, 'digit', str(i))
for w, v in zip(PLACES, [10, 100, 1000, 1000000]): add(w, 'place', str(v))
for w, g in DEM.items(): add(w, 'dem', g)
for w, g in ART.items(): add(w, 'art', g)
for w, g in {'punctum': 'decimal point', 'negatum': 'negative sign', 'ordo': 'ordinal marker'}.items(): add(w, 'numop', g)
add('codex', 'noun', 'code'); add('finis', 'noun', 'end')
for w, g in {'ego': 'I', 'tu': 'you', 'id': 'it', 'nos': 'we', 'vos': 'you (all)', 'ea': 'they'}.items(): add(w, 'pron', g)
for w in ('status', 'error', 'telos', 'hostis', 'minatio', 'nodus', 'locus', 'area', 'patrolium', 'iter', 'via',
          'signum', 'energia', 'periculum', 'accessus', 'connectio', 'nuntius', 'canalis', 'zona', 'copia', 'securitas'):
    if w not in words: add(w, 'noun')
# fix macron words
for w in list(words):
    if w != canon(w):
        words[canon(w)] = words.pop(w)

lex = {
    'version': '1.0',
    'canonical': 'lowercase ASCII (macrons stripped); punctuation: . , : ?',
    'verbs': verbs,
    'words': dict(sorted(words.items())),
}
json.dump(lex, open(OUT, 'w', encoding='utf8'), indent=1, ensure_ascii=False)
from collections import Counter
c = Counter(p for e in words.values() for p in e['pos'])
print(len(verbs), 'verbs;', len(words), 'words;', dict(c))