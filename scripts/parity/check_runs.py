#!/usr/bin/env python3
"""Require equal measurements and image hashes across repeated capture runs."""
import argparse
import json
from pathlib import Path
import sys


def check(first, second):
    def index(path):
        return {(r['scene'],r['theme'],r['scale']):r for r in json.loads(Path(path).read_text())['rows']}
    a, b = index(first), index(second)
    if a.keys() != b.keys():
        raise ValueError('scene sets differ')
    if any(row['status'] == 'error' for row in [*a.values(), *b.values()]):
        raise ValueError('capture errors cannot prove determinism')
    failures = []
    for key, row in a.items():
        fields = (['status','structure','structure_differences'] if row['status']=='structural' else
                  ['status','changed','total','bbox','regions','mean_channel_error','size','impro_sha256','reference_sha256'])
        changed = [field for field in fields if row.get(field) != b[key].get(field)]
        if changed:
            failures.append(dict(scene=key,fields=changed))
    if failures:
        raise ValueError(json.dumps(failures))
    return len(a)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('first')
    parser.add_argument('second')
    args = parser.parse_args()
    try:
        print(f'{check(args.first,args.second)} cases: identical measurements and hashes (graph structure only)')
    except ValueError as error:
        print(str(error),file=sys.stderr)
        sys.exit(1)
