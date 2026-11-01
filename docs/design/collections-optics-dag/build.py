#!/usr/bin/env python3
"""Build the offline roadmap view from its dated dependency snapshot. No packages."""
import argparse
import copy
import json
from pathlib import Path
from statistics import mean

ROOT = Path(__file__).resolve().parent
WIDTH, HEIGHT, GAP = 194, 88, 60


def analyze(data, nodes, incoming, outgoing, rank):
    owners = {}
    streams = set()
    for stream in data['workstreams']:
        if stream['id'] in streams:
            raise ValueError('Duplicate workstream identifier')
        streams.add(stream['id'])
        for key in stream['tasks']:
            if key in owners or key not in nodes:
                raise ValueError(f'Overlapping or unknown workstream membership: {key}')
            owners[key] = stream['id']
    if set(owners) != set(nodes):
        raise ValueError('Every task must have exactly one workstream')
    for key, node in nodes.items():
        node['workstream'] = owners[key]

    keys = sorted(nodes)
    reach = {k: set() for k in keys}
    for u in sorted(keys, key=lambda k: (-rank[k], k)):
        for v in outgoing[u]:
            reach[u].add(v)
            reach[u].update(reach[v])
    # Exact poset width: maximum matching in the bipartite reachability graph.
    # Both a maximum antichain and a same-size chain cover certify the result.
    right = {}

    def augment(u, visited):
        for v in sorted(reach[u]):
            if v in visited:
                continue
            visited.add(v)
            if v not in right or augment(right[v], visited):
                right[v] = u
                return True
        return False

    for u in keys:
        augment(u, set())
    left = {u: v for v, u in right.items()}
    zl, zr = set(keys) - set(left), set()
    queue = sorted(zl)
    while queue:
        u = queue.pop()
        for v in sorted(reach[u]):
            if left.get(u) == v or v in zr:
                continue
            zr.add(v)
            if v in right and right[v] not in zl:
                zl.add(right[v])
                queue.append(right[v])
    antichain = sorted(zl - zr)
    chains = []
    for start in sorted(set(keys) - set(right)):
        chain = [start]
        while chain[-1] in left:
            chain.append(left[chain[-1]])
        chains.append(chain)
    if (len(chains) != len(antichain)
            or sorted(k for c in chains for k in c) != keys
            or any(v in reach[u] for u in antichain for v in antichain)
            or any(b not in reach[a] for c in chains for a, b in zip(c, c[1:]))):
        raise ValueError('Invalid maximum-width certificate')
    waves = [[k for k in keys if rank[k] == i] for i in range(max(rank.values()) + 1)]
    paths = {}
    for k in sorted(keys, key=lambda k: (rank[k], k)):
        paths[k] = max((paths[p] for p in incoming[k]), key=lambda p: (len(p), p), default=[]) + [k]
    data['analysis'] = {
        'waves': waves,
        'maximumAntichain': antichain,
        'chainCover': chains,
        'longestPath': max(paths.values(), key=lambda p: (len(p), p)),
        'crossWorkstreamEdges': [
            {'source': e['source'], 'target': e['target'],
             'fromStream': owners[e['source']], 'toStream': owners[e['target']]}
            for e in data['edges'] if owners[e['source']] != owners[e['target']]
        ]
    }


def prepare(data):
    data = copy.deepcopy(data)
    nodes = {n['id']: n for n in data['nodes']}
    if len(nodes) != len(data['nodes']):
        raise ValueError('Duplicate task IDs')
    edges = data['edges']
    seen = set()
    incoming = {key: [] for key in nodes}
    outgoing = {key: [] for key in nodes}
    for e in edges:
        pair = (e['source'], e['target'])
        if pair in seen or pair[0] == pair[1] or not all(k in nodes for k in pair):
            raise ValueError(f'Duplicate, reflexive, or unbound dependency: {pair}')
        if e['relationship'] != 'must_finish_before':
            raise ValueError('Only must_finish_before edges may be rendered')
        seen.add(pair)
        incoming[pair[1]].append(pair[0])
        outgoing[pair[0]].append(pair[1])
    remaining = set(nodes)
    rank = {}
    while remaining:
        ready = sorted(k for k in remaining if all(p in rank for p in incoming[k]))
        if not ready:
            raise ValueError('Dependencies are cyclic')
        for key in ready:
            rank[key] = max((rank[p] + 1 for p in incoming[key]), default=0)
            remaining.remove(key)
    analyze(data, nodes, incoming, outgoing, rank)
    # Empty routing slots carry long edges through columns without crossing cards.
    layers = [[] for _ in range(max(rank.values()) + 1)]
    for key, depth in rank.items():
        layers[depth].append(key)
    previous = {k: [] for k in nodes}
    following = {k: [] for k in nodes}
    chains = []
    for index, edge in enumerate(edges):
        a, b = edge['source'], edge['target']
        chain = [a]
        for depth in range(rank[a] + 1, rank[b]):
            key = f'_route_{index}_{depth}'
            layers[depth].append(key)
            previous[key], following[key] = [], []
            chain.append(key)
        chain.append(b)
        for a, b in zip(chain, chain[1:]):
            previous[b].append(a)
            following[a].append(b)
        chains.append(chain)

    def positions():
        totals = [sum(HEIGHT + 30 if k in nodes else 28 for k in layer) for layer in layers]
        height = max(totals)
        ys = {}
        for layer, total in zip(layers, totals):
            y = (height - total) / 2 + 64
            for key in layer:
                size = HEIGHT + 30 if key in nodes else 28
                ys[key] = y + size / 2
                y += size
        return ys, height + 110

    def crossings(ys):
        total = 0
        for left in layers[:-1]:
            segments = [(a, b) for a in left for b in following[a]]
            for i, (a, b) in enumerate(segments):
                for c, d in segments[i + 1:]:
                    if (ys[a] - ys[c]) * (ys[b] - ys[d]) < 0:
                        total += 1
        return total

    best = None
    for sweep in range(16):
        order = range(1, len(layers)) if sweep % 2 == 0 else range(len(layers) - 2, -1, -1)
        neighbors = previous if sweep % 2 == 0 else following
        for depth in order:
            ys, _ = positions()
            layers[depth].sort(key=lambda k: mean(ys[p] for p in neighbors[k]) if neighbors[k] else ys[k])
        ys, height = positions()
        score = crossings(ys)
        if best is None or score < best[0]:
            best = (score, copy.deepcopy(layers))
    layers = best[1]
    ys, height = positions()
    columns = {k: i for i, layer in enumerate(layers) for k in layer}
    for key, node in nodes.items():
        node.update(x=44 + rank[key] * (WIDTH + GAP), y=round(ys[key] - HEIGHT / 2, 2),
                    depth=rank[key], width=WIDTH, height=HEIGHT)
        node['entry'] = not incoming[key]
    for edge, chain in zip(edges, chains):
        source, target = nodes[edge['source']], nodes[edge['target']]
        # Fan-in/out use separate ports, keeping arrowheads visible at joins.
        out = sorted(outgoing[source['id']], key=lambda k: ys[k])
        inc = sorted(incoming[target['id']], key=lambda k: ys[k])
        sy = ys[source['id']] + (out.index(target['id']) - (len(out) - 1) / 2) * 9
        ty = ys[target['id']] + (inc.index(source['id']) - (len(inc) - 1) / 2) * 9
        x, y = source['x'] + WIDTH, sy
        parts = [f'M {x:.2f} {y:.2f}']
        for key in chain[1:]:
            nx = 44 + columns[key] * (WIDTH + GAP)
            ny = ty if key == target['id'] else ys[key]
            mid = (x + nx) / 2
            parts.append(f'C {mid:.2f} {y:.2f} {mid:.2f} {ny:.2f} {nx:.2f} {ny:.2f}')
            x, y = nx, ny
            if key != target['id']:
                x += WIDTH
                parts.append(f'H {x:.2f}')
        edge['path'] = ' '.join(parts)
    data['layout'] = {'width': 88 + len(layers) * WIDTH + (len(layers) - 1) * GAP,
                      'height': height, 'columns': len(layers), 'columnStep': WIDTH + GAP}
    return data


def render():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true', help='Check generated HTML without writing it')
    args = parser.parse_args()
    data = prepare(json.loads((ROOT / 'snapshot.json').read_text()))
    encoded = json.dumps(data, ensure_ascii=False, separators=(',', ':')).replace('&', '\\u0026').replace('<', '\\u003c')
    template = (ROOT / 'page.template.html').read_text()
    if template.count('__ROADMAP_DATA__') != 1:
        raise ValueError('Expected exactly one data insertion point')
    content = template.replace('__ROADMAP_DATA__', encoded)
    path = ROOT / 'index.html'
    if args.check:
        if not path.exists() or path.read_text() != content:
            raise SystemExit('Generated HTML is stale; run build.py')
    else:
        path.write_text(content)
    print(f'{len(data["nodes"])} tasks; {len(data["edges"])} prerequisite edges; acyclic; ' + ('HTML current' if args.check else 'HTML built'))


if __name__ == '__main__':
    render()
