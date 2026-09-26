# System Latin toolkit (spec v1.0)

Machine-readable companions to `system_latin_systemized.md` (spec v1.2).

| File | Purpose |
|---|---|
| `sl_grammar.ebnf` | Formal grammar (EBNF) with side conditions |
| `sl_lexicon.json` | 109 verb stems (with valency) and 356 words with part of speech |
| `sl_parser.py` | Reference tokenizer, morphological analyzer, parser, and validator |
| `sl_tests.txt` | 394 test lines: spec examples, extra valid sentences, invalid sentences with expected error codes, numerals |
| `test_ast.py` | 17 structural tests (checks the parse tree, not just acceptance) |
| `build_lexicon.py` | Regenerates the lexicon from the spec markdown |
| `sl_pack.py` | Vocabulary pack linter, hasher, and loader (spec Section XLIX) |
| `example_pack.json` | A valid sample pack with verbs, words, and phrase templates |
| `test_canonical.py` | Canonical-form tests (idempotent, tree-preserving) |
| `test_packs.py` | 18 pack linter tests |

## Quick start

    python3 sl_parser.py "Si hostis detectum, tunc defenda."      # prints the parse tree as JSON
    python3 sl_parser.py --strict "Nuntius codex unum: ab agent codex quattuor, ad omnia. Redia."   # envelope required
    python3 sl_parser.py --tests sl_tests.txt                       # 394 passed, 0 failed
    python3 test_ast.py                                             # 17 / 17
    python3 sl_parser.py --canonical "RESUMA  patrōlium ."       # resuma patrolium.
    python3 sl_pack.py lint example_pack.json                       # validate a vocabulary pack
    python3 test_canonical.py && python3 test_packs.py
    python3 build_lexicon.py                                        # rebuild sl_lexicon.json

Exit code is 0 when a message is valid. Errors carry a code (`E_UNKNOWN`, `E_NOUNS`, `E_ELSE`, ...); see Section XLVII of the spec.

## What is verified

- 3,453 surface forms are generated from the verb paradigm and lexicon. No form is shared by two verbs, and no form splits into two stem-plus-ending readings.
- Only ten forms carry more than one part of speech (`ante`, `infra`, `longe`, `post`, `prope`, `retro`, `supra` as preposition or adverb; `minus` as adjective or comparator; `negatum` as status or negative sign; `quando` as question word or rule word). Each is resolved by position in the grammar.
- All 121 example lines in the spec parse, and the three envelope examples parse in strict mode.
- v1.1 adds the message envelope (spec Section XLVIII) and a strict mode that requires it.
- Systemization pass 2: all adjectives regularized to invariant **-um**, and manner adverbs are now derived from adjectives (**-um → -iter**) rather than a closed irregular list.
- v1.2 adds canonical text (the bytes to sign), message limits (`E_LIMIT`), and vocabulary packs with a linter. Host-per-world authority, frames, and the handshake are specified in Section XLIX; the toolkit implements the language-side checks only, not the network layer.

## Known limits (v1.0)

- No nested rules, no multi-sentence rule bodies, no user-defined names, quoting, or comments.
- Signing, encryption, host migration, and delivery status belong to the transport layer and are not implemented here. Envelope fields are fixed.
- Verb valency is only transitive/intransitive; it does not check what kind of object a verb takes.
- Attributive adjectives cannot be combined with a predicate adjective in one clause (`hostis possibile activum` reads the last adjective as the predicate).
