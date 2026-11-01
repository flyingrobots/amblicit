"""Validate graph certificates and the ownership partition, without third-party code."""
import copy
import json
from pathlib import Path
import unittest

from build import prepare


class AnalysisChecks(unittest.TestCase):
    def setUp(self):
        self.raw = json.loads((Path(__file__).parent / 'snapshot.json').read_text())

    def test_partition_antichains_and_width_certificates(self):
        data = prepare(self.raw)
        ids = {n['id'] for n in data['nodes']}
        after = {k: set() for k in ids}
        for e in data['edges']:
            after[e['source']].add(e['target'])
        for via in sorted(ids):
            for node in ids:
                if via in after[node]:
                    after[node] |= after[via]
        memberships = [k for s in data['workstreams'] for k in s['tasks']]
        self.assertCountEqual(memberships, ids)
        wave_members = [k for w in data['analysis']['waves'] for k in w]
        self.assertCountEqual(wave_members, ids)
        assigned = {k: i for i, w in enumerate(data['analysis']['waves']) for k in w}
        for a in ids:
            for b in after[a]:
                self.assertLess(assigned[a], assigned[b])
        a = data['analysis']['maximumAntichain']
        self.assertEqual(len(a), 9)
        self.assertTrue(all(v not in after[u] for u in a for v in a))
        chains = data['analysis']['chainCover']
        self.assertEqual(len(chains), len(a))
        self.assertCountEqual([k for c in chains for k in c], ids)
        for chain in chains:
            self.assertTrue(all(b in after[a] for a, b in zip(chain, chain[1:])))
        self.assertEqual(max(map(len, data['analysis']['waves'])), 8)
        path = data['analysis']['longestPath']
        edge_set = {(e['source'], e['target']) for e in data['edges']}
        self.assertEqual(len(path), 9)
        self.assertTrue(all(pair in edge_set for pair in zip(path, path[1:])))

    def test_overlapping_workstream_is_rejected(self):
        self.raw['workstreams'][1]['tasks'].append('list_core')
        with self.assertRaisesRegex(ValueError, 'workstream'):
            prepare(self.raw)

    def test_generation_width_is_not_poset_width(self):
        raw = {'nodes': [{'id': k} for k in 'abcd'],
               'edges': [{'source': 'a', 'target': k, 'relationship': 'must_finish_before'}
                         for k in 'bc'],
               'workstreams': [{'id': 'one', 'tasks': list('abcd')}]}
        data = prepare(raw)
        self.assertEqual(max(map(len, data['analysis']['waves'])), 2)
        self.assertEqual(len(data['analysis']['maximumAntichain']), 3)
        self.assertEqual(len(data['analysis']['chainCover']), 3)

    def test_unassigned_task_is_rejected(self):
        self.raw['workstreams'][0]['tasks'].remove('list_core')
        with self.assertRaisesRegex(ValueError, 'workstream'):
            prepare(self.raw)

    def test_cycles_and_nonprecedence_edges_are_rejected(self):
        for edge in [
            {'source': 'retire_create', 'target': 'model', 'relationship': 'must_finish_before'},
            {'source': 'model', 'target': 'list_core', 'relationship': 'resource_contention'},
        ]:
            with self.subTest(edge=edge):
                raw = copy.deepcopy(self.raw)
                raw['edges'].append(edge)
                with self.assertRaises(ValueError):
                    prepare(raw)


if __name__ == '__main__':
    unittest.main()
