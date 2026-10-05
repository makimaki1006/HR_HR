"""Immutable snapshot retry/size/scope tests using synthetic data and CLI."""
import copy
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from job_copy_drive_pilot import CliError, PilotError
from job_copy_drive_snapshot import DriveSnapshotPilot, snapshot_bytes, SNAPSHOT_LIMIT
from job_copy_drive_manifest import ServiceReadbackCli

FOLDER = 'synthetic_folder_12345'
FILE = 'synthetic_snapshot_12345'


class MockCli:
    def __init__(self):
        self.calls = []
        self.metadata = None
        self.raw = None
        self.loss = None
        self.corrupt = False
        self.before = None

    def call(self, method, params, body=None, upload=None, output=None):
        self.calls.append((method, params, body))
        if method == 'generateIds': return {'ids': [FILE]}
        if method == 'create':
            if self.before: self.before(body, upload)
            if self.loss == 'before':
                self.loss = None
                raise CliError(reason='cli_outcome_unknown')
            self.raw = Path(upload).read_bytes()
            self.metadata = {**body, 'size': str(len(self.raw)), 'trashed': False,
                             'capabilities': {'canDownload': True}}
            if self.loss == 'after':
                self.loss = None
                raise CliError(reason='cli_outcome_unknown')
            return {}
        if params['fileId'] == FOLDER:
            return {'id': FOLDER, 'mimeType': 'application/vnd.google-apps.folder',
                    'trashed': False, 'capabilities': {'canAddChildren': True}}
        if self.metadata is None: raise CliError(404)
        if output is not None:
            Path(output).write_bytes(b'corrupt' if self.corrupt else self.raw)
            return {}
        return copy.deepcopy(self.metadata)

    def count(self, method): return sum(row[0] == method for row in self.calls)


class SnapshotTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.source = self.root / 'synthetic.json'
        self.value = {'schemaVersion': 1, 'capture_bundle': {'jobs': []}, 'results': []}
        self.raw = json.dumps(self.value, indent=2).encode('utf-8')
        self.source.write_bytes(self.raw)
        self.output = self.root / 'receipts'
        self.cli = MockCli()
        self.config = {'schemaVersion': 1, 'folderId': FOLDER, 'identityFolderId': FOLDER}

    def pilot(self): return DriveSnapshotPilot(self.config, self.output, self.cli)
    def receipt(self):
        return json.loads(next(p for p in self.output.glob('*.json') if not p.name.endswith('.review.json')).read_bytes())

    def test_original_bytes_and_reserved_receipt_precede_create_and_repeat_reuses_id(self):
        def before(body, upload):
            self.assertEqual((self.receipt()['file_id'], self.receipt()['state']), (FILE, 'sending'))
            self.assertEqual(Path(upload).read_bytes(), self.raw)
            self.assertEqual(body['appProperties']['job_copy_kind'], 'review_snapshot')
        self.cli.before = before
        result = self.pilot().ensure_snapshot(self.source, True)
        self.assertEqual(set(result), {'id', 'sha256', 'bytes', 'readback_hash_matches'})
        self.assertEqual(result['bytes'], len(self.raw))
        self.assertTrue(result['readback_hash_matches'])
        self.pilot().ensure_snapshot(self.source)
        self.assertEqual((self.cli.count('generateIds'), self.cli.count('create')), (1, 1))

    def test_response_loss_after_create_recovers_without_second_create(self):
        self.cli.loss = 'after'
        self.assertEqual(self.pilot().ensure_snapshot(self.source, True)['id'], FILE)
        self.assertEqual(self.cli.count('create'), 1)

    def test_response_loss_before_create_restarts_with_persisted_id_only(self):
        self.cli.loss = 'before'
        with self.assertRaises(CliError): self.pilot().ensure_snapshot(self.source, True)
        self.assertEqual(self.receipt()['state'], 'uncertain')
        self.pilot().ensure_snapshot(self.source, True)
        self.assertEqual(self.cli.count('generateIds'), 1)
        self.assertEqual([row[2]['id'] for row in self.cli.calls if row[0] == 'create'], [FILE, FILE])

    def test_bad_raw_readback_is_failed_and_never_reuploads_existing_id(self):
        self.cli.corrupt = True
        with self.assertRaisesRegex(PilotError, '^download_hash_mismatch$'):
            self.pilot().ensure_snapshot(self.source, True)
        self.assertEqual(self.receipt()['state'], 'verification_failed')
        self.cli.corrupt = False
        self.pilot().ensure_snapshot(self.source)
        self.assertEqual(self.cli.count('create'), 1)
        self.assertEqual(list(self.output.glob('*.download')), [])

    def test_parent_mime_size_properties_or_permission_mismatch_fail(self):
        self.pilot().ensure_snapshot(self.source, True)
        for key, value in [('parents', ['foreign_folder_12345']), ('mimeType', 'image/jpeg'),
                           ('size', '0'), ('appProperties', {}), ('capabilities', {'canDownload': False})]:
            old = self.cli.metadata[key]
            self.cli.metadata[key] = value
            with self.subTest(key=key), self.assertRaisesRegex(PilotError, '^remote_metadata_mismatch$'):
                self.pilot().ensure_snapshot(self.source, True)
            self.cli.metadata[key] = old
        self.assertEqual(self.cli.count('create'), 1)

    def test_schema_decode_and_overlimit_are_rejected_before_cli(self):
        for raw in (b'\xff', b'[]', b'{"schemaVersion":true,"capture_bundle":{"jobs":[]},"results":[]}',
                    b'{"schemaVersion":1,"capture_bundle":{"jobs":{}},"results":[]}'):
            self.source.write_bytes(raw)
            with self.subTest(raw=raw), self.assertRaises(PilotError): self.pilot().ensure_snapshot(self.source, True)
        self.source.write_bytes(self.raw)
        with patch('job_copy_drive_snapshot.SNAPSHOT_LIMIT', len(self.raw) - 1), self.assertRaisesRegex(PilotError, '^snapshot_size_invalid$'):
            self.pilot().ensure_snapshot(self.source, True)
        self.assertEqual(self.cli.calls, [])

    def test_above_image_limit_snapshot_valid_and_service_limit_is_explicit_32mib(self):
        large = {**self.value, 'syntheticPadding': 'x' * (5 * 1024 * 1024)}
        self.source.write_bytes(json.dumps(large).encode('utf-8'))
        self.assertGreater(len(snapshot_bytes(self.source)), 5 * 1024 * 1024)
        class Credentials: pass
        reader = ServiceReadbackCli(self.cli, None, credentials=Credentials(), max_bytes=SNAPSHOT_LIMIT)
        self.assertEqual(reader.max_bytes, 32 * 1024 * 1024)
        for value in (0, SNAPSHOT_LIMIT + 1):
            with self.assertRaises(PilotError):
                ServiceReadbackCli(self.cli, None, credentials=Credentials(), max_bytes=value)

    def test_readonly_first_run_and_changed_local_evidence_cannot_reserve_or_replace(self):
        with self.assertRaisesRegex(PilotError, '^upload_flag_required$'):
            self.pilot().ensure_snapshot(self.source)
        self.assertEqual(self.cli.count('generateIds'), 0)
        result = self.pilot().ensure_snapshot(self.source, True)
        (self.output / (result['sha256'] + '.review.json')).write_bytes(b'changed')
        with self.assertRaisesRegex(PilotError, '^original_evidence_missing_or_changed$'):
            self.pilot().ensure_snapshot(self.source, True)
        self.assertEqual(self.cli.count('create'), 1)

    def test_wrong_identity_cannot_reserve_duplicate_snapshot_in_same_receipt_directory(self):
        self.pilot().ensure_snapshot(self.source, True)
        self.config['identityFolderId'] = 'synthetic_wrong_identity_12345'
        self.cli.calls.clear()
        with self.assertRaisesRegex(PilotError, '^receipt_identity_conflict$'):
            self.pilot().ensure_snapshot(self.source, True)
        self.assertEqual(self.cli.count('generateIds'), 0)
        self.assertEqual(self.cli.count('create'), 0)
        self.assertEqual(self.receipt()['folder_id'], FOLDER)

    def test_utf8_bom_rejected_before_reserving_or_creating(self):
        self.source.write_bytes(b'\xef\xbb\xbf' + self.raw)
        with self.assertRaisesRegex(PilotError, '^snapshot_utf8_bom_unsupported$'):
            self.pilot().ensure_snapshot(self.source, True)
        self.assertEqual(self.cli.calls, [])


if __name__ == '__main__': unittest.main()
