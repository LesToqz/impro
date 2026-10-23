"""Observable measurements and their explicit tolerances."""
import json


def compare(reference, impro, out):
    rows=[]
    for name,r in reference['steps'].items():
        i=impro['steps'].get(name)
        if i is None:
            raise RuntimeError('missing native step '+name)
        rows.append(dict(behaviour=name, reference=r['state']['nodes'],
                         impro=i['state']['nodes'],verdict='same' if r['state']['nodes']==i['state']['nodes'] else 'different'))
    (out/'comparison.json').write_text(json.dumps(rows,indent=2))
    print('behaviour | reference | impro | verdict')
    for row in rows:
        print(' | '.join(str(row[k]) for k in ('behaviour','reference','impro','verdict')))
