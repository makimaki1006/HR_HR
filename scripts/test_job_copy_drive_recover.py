"""Recovery uses synthetic remote manifests; never real OAuth or Drive calls."""
import copy
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from job_copy_drive_recover import GwsManifestList, list_candidates, recover, MANIFEST_LIMIT
from job_copy_drive_manifest import validate_manifest
from job_copy_drive_pilot import PilotError
from job_copy_snapshot_store import canonical

FOLDER = 'synthetic_folder_12345'
FILE = 'synthetic_manifest_12345'


class Pages:
    def __init__(self, pages):
        self.pages = pages
        self.cursors = []
    def page(self, cursor):
        self.cursors.append(cursor)
        return self.pages[len(self.cursors) - 1]


class RecoveryTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.output = self.root / 'recovered.json'
        self.manifest = {'schemaVersion': 1, 'listingId': '30', 'companyIds': ['10'],
            'observedAt': '2026-10-05T01:02:03Z', 'images': [{'slot': 1,
            'fileId': 'synthetic_image_12345', 'sha256': 'a' * 64, 'mimeType': 'image/png', 'size': 15}]}
        self.manifest['operationId'] = hashlib.sha256(canonical({'folderId': FOLDER, 'manifest': self.manifest})).hexdigest()
        self.raw = canonical(self.manifest)
        self.metadata = {'id': FILE, 'mimeType': 'application/json', 'parents': [FOLDER], 'trashed': False,
            'size': str(len(self.raw)), 'capabilities': {'canDownload': True}, 'appProperties': {
            'job_copy_kind': 'manifest', 'job_copy_sha256': hashlib.sha256(self.raw).hexdigest(),
            'job_copy_operation': self.manifest['operationId'], 'job_copy_sync': 'pending'}}
        outer = self
        class Readback:
            calls = []
            def call(self, method, params, **kwargs):
                self.calls.append((method, params))
                if method != 'get': raise AssertionError('writes forbidden')
                Path(kwargs['output']).write_bytes(outer.raw)
        self.readback = Readback()

    def pages(self):
        return Pages([{'files': [self.metadata]}])

    def test_reconstructs_without_any_receipt_and_pending_is_candidate(self):
        result = recover(FOLDER, self.pages(), self.readback, self.output)
        self.assertEqual(result['manifest_candidates'], 1)
        row = json.loads(self.output.read_bytes())['results'][0]
        self.assertEqual(row['listing_id'], '30')
        self.assertEqual(row['manifest_file_id'], FILE)
        self.assertEqual(row['create_attempts'], 0)
        self.assertEqual(row['sync_state'], 'candidate_requires_hubspot_comparison')
        self.assertTrue(row['readback_hash_matches'])
        self.assertEqual(list(self.root.glob('*.download')), [])

    def test_cursor_paging_and_duplicate_ids_are_strict(self):
        pages = Pages([{'files': [], 'nextPageToken': 'cursor1'}, {'files': [self.metadata]}])
        self.assertEqual(len(list_candidates(pages, FOLDER)), 1)
        self.assertEqual(pages.cursors, [None, 'cursor1'])
        with self.assertRaisesRegex(PilotError, 'duplicate_manifest_id'):
            list_candidates(Pages([{'files': [self.metadata], 'nextPageToken': 'cursor1'},
                                   {'files': [self.metadata]}]), FOLDER)

    def test_loops_malformed_cursors_incomplete_and_page_cap_fail(self):
        scenarios = [([{'files': [], 'nextPageToken': 'same'}] * 2, 'invalid_manifest_cursor'),
            ([{'files': [], 'nextPageToken': 123}], 'invalid_manifest_cursor'),
            ([{'files': [], 'nextPageToken': ''}], 'invalid_manifest_cursor'),
            ([{'files': [], 'incompleteSearch': True}], 'incomplete_manifest_listing'),
            ([{'files': [], 'nextPageToken': str(i)} for i in range(10)], 'manifest_listing_page_limit'),
            ([{'files': [self.metadata] * 101}], 'invalid_manifest_page')]
        for values, reason in scenarios:
            with self.subTest(reason=reason), self.assertRaisesRegex(PilotError, reason):
                list_candidates(Pages(values), FOLDER)

    def test_folder_mime_hash_size_and_download_permission_are_enforced(self):
        invalid = [('parents', ['foreign_folder_12345']), ('mimeType', 'image/png'),
            ('trashed', True), ('size', str(MANIFEST_LIMIT + 1)), ('size', 'bad'),
            ('capabilities', {'canDownload': False}), ('appProperties', {'job_copy_kind': 'other'})]
        for key, value in invalid:
            metadata = {**self.metadata, key: value}
            with self.subTest(key=key), self.assertRaisesRegex(PilotError, 'invalid_manifest_metadata'):
                recover(FOLDER, Pages([{'files': [metadata]}]), self.readback, self.output)
        self.assertEqual(self.readback.calls, [])

    def test_failed_hash_does_not_replace_previous_complete_output(self):
        self.output.write_bytes(b'previous verified output')
        self.raw = b'x' * len(self.raw)
        with self.assertRaisesRegex(PilotError, 'manifest_hash_mismatch'):
            recover(FOLDER, self.pages(), self.readback, self.output)
        self.assertEqual(self.output.read_bytes(), b'previous verified output')
        self.assertEqual(list(self.root.glob('*.download')), [])

    def test_manifest_limits_and_unknown_fields_match_rust_boundary(self):
        for change in ({'companyIds': [str(i + 1) for i in range(101)]}, {'unknown': True}):
            value = {**self.manifest, **change}
            with self.assertRaises(PilotError): validate_manifest(value)
        value = copy.deepcopy(self.manifest)
        value['images'][0]['slot'] = 101
        with self.assertRaisesRegex(PilotError, 'invalid_image_slot'): validate_manifest(value)
        value = copy.deepcopy(self.manifest)
        value['images'] *= 101
        with self.assertRaisesRegex(PilotError, 'invalid_manifest_images'): validate_manifest(value)

    def test_operation_hash_and_metadata_operation_must_match_content(self):
        self.metadata['appProperties']['job_copy_operation'] = 'b' * 64
        with self.assertRaisesRegex(PilotError, 'manifest_operation_mismatch'):
            recover(FOLDER, self.pages(), self.readback, self.output)
        self.assertFalse(self.output.exists())

    def test_listing_cli_query_is_exact_folder_kind_and_read_only(self):
        class Cli: executable = 'synthetic-gws'
        with patch('job_copy_drive_recover.subprocess.run', return_value=subprocess.CompletedProcess([], 0, '{"files":[]}', '')) as run:
            GwsManifestList(FOLDER, Cli()).page('cursor')
        argv = run.call_args.args[0]
        self.assertEqual(argv[1:4], ['drive', 'files', 'list'])
        params = json.loads(argv[argv.index('--params') + 1])
        self.assertIn("'" + FOLDER + "' in parents", params['q'])
        self.assertIn("key='job_copy_kind' and value='manifest'", params['q'])
        self.assertEqual(params['pageSize'], 100)
        self.assertEqual(params['pageToken'], 'cursor')
        self.assertNotIn('shell', run.call_args.kwargs)

    def test_recovery_after_completed_move_uses_old_identity_and_new_parent(self):
        physical = 'synthetic_new_folder_12345'
        self.metadata['parents'] = [physical]
        result = recover(physical, self.pages(), self.readback, self.output, identity_folder_id=FOLDER)
        self.assertEqual(result['manifest_candidates'], 1)
        row = json.loads(self.output.read_bytes())['results'][0]
        self.assertEqual(row['manifest_file_id'], FILE)
        self.assertEqual(row['operation_id'], self.manifest['operationId'])
        self.assertEqual(row['sha256'], hashlib.sha256(self.raw).hexdigest())

    def test_recovery_wrong_identity_or_unmoved_manifest_fails_closed(self):
        physical = 'synthetic_new_folder_12345'
        with self.assertRaisesRegex(PilotError, '^invalid_manifest_metadata$'):
            recover(physical, self.pages(), self.readback, self.output, identity_folder_id=FOLDER)
        self.metadata['parents'] = [physical]
        with self.assertRaisesRegex(PilotError, '^manifest_operation_mismatch$'):
            recover(physical, self.pages(), self.readback, self.output)
        self.assertFalse(self.output.exists())


if __name__ == '__main__':
    unittest.main()
