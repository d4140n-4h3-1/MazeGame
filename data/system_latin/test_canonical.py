#!/usr/bin/env python3
"""Canonical form: one spelling per message, idempotent, AST-preserving."""
import sl_parser as sp

CASES = {
    "  RESUMA   patr\u014dlium ." : "resuma patrolium.",
    "Si hostis detectum , tunc  defenda ." : "si hostis detectum, tunc defenda.",
    "Telos tu:eliminare   minatio.": "telos tu: eliminare minatio.",
    "UBI hostis ?": "ubi hostis?",
    "Nuntius codex unum: ab agent codex quattuor,ad omnia.Redia.": "nuntius codex unum: ab agent codex quattuor, ad omnia. redia.",
}
bad = 0
for raw, want in CASES.items():
    got = sp.canonical_text(raw)
    if got != want: bad += 1; print('FAIL', repr(raw), '->', repr(got), 'expected', repr(want))
# every valid line in the corpus: idempotent, ASCII, and the parse tree is unchanged
n = 0
for line in open('sl_tests.txt', encoding='utf8'):
    if line.startswith('OK'):
        text = line.split(' | ', 1)[1].rstrip('\n')
        strict = line.startswith('OK+ENV')
        c = sp.canonical_text(text, strict)
        assert c == sp.canonical_text(c, strict), ('not idempotent', text)
        assert c.isascii() and c == c.strip() and '  ' not in c, ('not canonical', c)
        assert sp.parse_message(c, strict) == sp.parse_message(text, strict), ('tree changed', text)
        n += 1
print(f'{n} corpus lines checked;', 'FAIL' if bad else 'all canonical cases pass')
raise SystemExit(1 if bad else 0)
