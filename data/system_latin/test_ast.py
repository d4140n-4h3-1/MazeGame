#!/usr/bin/env python3
"""Structural tests: the parser must build the intended tree, not just accept the sentence."""
import sl_parser as sp

def ast(text): 
    ok, r = sp.check(text); assert ok, (text, r); return r

def t(name, cond):
    print(('PASS ' if cond else 'FAIL ') + name); return cond

results = []
r = ast('Non redia donec id terminatum.')[0]
results.append(t('negated command with until-clause', r['type']=='command' and r['neg'] and r['until']['pred']['word']=='terminatum'))
r = ast('Telos tu: eliminare minatio.')[0]
results.append(t('purpose: dependent pronoun and infinitive object', r['topic']['dep']=='tu' and r['body']['verb']=='elimin' and r['body']['object']['head']=='minatio'))
r = ast('Scana area patrolium.')[0]
results.append(t('compound object is one noun phrase', r['object']['head']=='area' and r['object']['dep']=='patrolium'))
r = ast('Hostis scanat nodus.')[0]
results.append(t('subject-verb-object roles', r['subject']['head']=='hostis' and r['object']['head']=='nodus' and r['person']=='it'))
r = ast('Zeto hostis.')[0]
results.append(t('self-report has object and no subject', r['subject'] is None and r['object']['head']=='hostis' and r['person']=='I'))
r = ast('Hostis activum.')[0]
results.append(t('final adjective is the predicate', r['type']=='status' and r['pred']['word']=='activum' and not r['subject']['adj']))
r = ast('Hostis possibilum sentatur.')[0]
results.append(t('adjective before passive verb is attributive', r['type']=='finite' and r['subject']['adj']==['possibilum'] and r['kind']=='pass'))
r = ast('Si hostis detectum et energia insufficians, retraha.')[0]
results.append(t('and-condition', r['cond']['type']=='and' and len(r['cond']['args'])==2))
r = ast('Si hostis detectum aut periculum detectum et energia insufficians, retraha.')[0]
results.append(t('et binds tighter than aut', r['cond']['type']=='or' and r['cond']['args'][1]['type']=='and'))
m = ast('Si nodus perdatum, reconnecta. Aliter si hostis detectum, defenda. Aliter, progreda.')
results.append(t('else chain', [x['type'] for x in m]==['rule','else_if','else']))
r = ast('Quando hostis detectum?')[0]
results.append(t('quando with ? is a question', r['type']=='question'))
r = ast('Quando hostis detectum, defenda.')[0]
results.append(t('quando with comma is a trigger', r['type']=='rule' and r['kind']=='quando'))
r = ast('Verifica utrum hostis detectum.')[0]
results.append(t('embedded question', r['embedded']['qw']=='utrum'))
r = ast('Duo mille duo decem sex hostis detectum.')[0]
results.append(t('quantity value inside noun phrase', r['subject']['quantity']['value']==2026))
r = ast('Ad ubi progredamus?')[0]
results.append(t('preposition + question word', r['prep']=='ad' and r['qw']=='ubi'))
r = ast('Energia minus decem.')[0]
results.append(t('comparison', r['type']=='comparison' and r['op']=='minus' and r['right']['quantity']['value']==10))
r = ast('Error minus.')[0]
results.append(t('minus as adjective', r['type']=='status' and r['pred']['word']=='minus'))
ok, m = sp.check('Nuntius codex unum duo tria: ab agent codex quattuor, ad omnia volator, prioritas altum, responsum codex unum. Redia.', True)
e = m[0]
results.append(t('envelope fields', e['type']=='envelope' and e['id']=='123' and e['priority']=='altum' and e['reply_to']=='1' and e['from']['dep_code']=='4' and e['to']['quantity']=='omnia'))
results.append(t('envelope then body', len(m)==2 and m[1]['type']=='command'))
r = ast('Sed hostis detectum.')[0]
results.append(t('sed does not get swallowed into the subject noun phrase', r['subject']['head']=='hostis' and 'dep' not in r['subject'] and r.get('connectors')==['sed']))
r = ast('Sed tamen esant simia.')[0]
results.append(t('sed and tamen both strip as connectors, subject stays implicit', r['subject'] is None and r.get('connectors')==['sed','tamen']))
print(sum(results),'/',len(results),'structural tests passed')
raise SystemExit(0 if all(results) else 1)
