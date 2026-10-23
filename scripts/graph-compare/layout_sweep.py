"""Generated layout observations; each implementation places its own graph."""
import json
import math
import os
from pathlib import Path
import statistics


def run(args, drive):
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    rows = []
    for shape, n in [('hub-spokes', 60), ('chain', 60), ('islands', 90), ('large-islands', 3000)]:
        root = out/shape/'fixture'
        root.mkdir(parents=True, exist_ok=True)
        if shape == 'hub-spokes':
            files = {'hub/centre.md': '\n'.join(f'[[spoke-{i}]]' for i in range(29))}
            files.update({f'hub/spoke-{i}.md': '[[centre]]\n' for i in range(29)})
            files.update({f'cluster/ring-{i}.md': f'[[ring-{(i+1)%30}]]\n' for i in range(30)})
        elif shape == 'chain':
            files = {f'chain/link-{i}.md': f'[[link-{i+1}]]\n' if i<59 else 'end\n' for i in range(n)}
        else:
            groups = 3 if n == 90 else 30
            size = n//groups
            files = {f'group-{g}/node-{i}.md': f'[[node-{(i+1)%size}]]\n'
                     for g in range(groups) for i in range(size)}
        for name, text in files.items():
            path = root/name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text)
            os.utime(path,(1700000000,1700000000))
        actions = root.parent/'actions.json'
        actions.write_text(json.dumps([dict(op='capture',name='default',png=True)]))
        env = dict(os.environ, XDG_CONFIG_HOME=str(root.parent/'config'),
                   XDG_CACHE_HOME=str(root.parent/'cache'), GIO_USE_VFS='local')
        for side in ('reference','impro'):
            for repeat in (False,True):
                destination = root.parent/(side+('-repeat' if repeat else ''))
                if side == 'reference':
                    command = ['/usr/bin/python3','-I','-B',str(Path(__file__).with_name('reference.py')),
                               '--reference',str(args.reference.resolve()),'--root',str(root),
                               '--actions',str(actions),'--out',str(destination)]
                else:
                    command = [str(args.impro_bin.resolve()),str(root),str(actions),str(destination)]
                drive(command,destination,env)
            measurements = json.loads((root.parent/side/'measurements.json').read_text())
            again = json.loads((root.parent/(side+'-repeat')/'measurements.json').read_text())
            nodes = measurements['steps']['default']['nodes']
            positions = {key:value['world'] for key,value in nodes.items()}
            moved = max(math.dist(pos,again['steps']['default']['nodes'][key]['world'])
                        for key,pos in positions.items())
            # Skip the quadratic nearest-neighbour diagnostic on the large
            # fixture; centroid spread and repeat determinism remain measured.
            values = list(positions.values())
            if n <= 90:
                nearest = statistics.median(min(math.dist(x,y) for y in values if y is not x) for x in values)
            else:
                nearest = None
            groups = {}
            for key,pos in positions.items():
                groups.setdefault(str(Path(key).parent),[]).append(pos)
            centres = [[statistics.mean(p[axis] for p in points) for axis in (0,1)]
                       for points in groups.values()]
            separations = [math.dist(a,b) for i,a in enumerate(centres) for b in centres[i+1:]]
            rows.append(dict(shape=shape,side=side,nodes=len(nodes),
                             settle_ms=round(measurements['initial_settle_ms']),
                             nearest=round(nearest,2) if nearest is not None else None,
                             cluster_separation=round(statistics.median(separations),2) if separations else None,
                             repeat_max=round(moved,5)))
            print(rows[-1],flush=True)
        (out/'layout-results.json').write_text(json.dumps(rows,indent=2))
    return rows
