"""Synthetic-only manifest integrity and retry checks; no real CLI or OAuth."""
import copy
import hashlib
import io
import json
from pathlib import Path
import tempfile
import unittest

from PIL import Image

from job_copy_drive_manifest import build_manifests, DriveManifestPilot, RetryReadCli, ServiceReadbackCli
from job_copy_drive_pilot import CliError, PilotError

FOLDER = 'synthetic_folder_12345'
IMAGE = 'synthetic_image_12345'
MANIFEST = 'synthetic_manifest_12345'


class MockCli:
    def __init__(self, raw):
        digest = hashlib.sha256(raw).hexdigest()
        self.files = {IMAGE: (raw, {'id': IMAGE, 'mimeType': 'image/png', 'size': str(len(raw)),
            'parents': [FOLDER], 'trashed': False, 'capabilities': {'canDownload': True},
            'appProperties': {'job_copy_sha256': digest}})}
        self.calls = []
        self.loss = None
        self.corrupt = None
        self.before_create = None
        self.folder_id = FOLDER

    def call(self, method, params, body=None, upload=None, output=None):
        self.calls.append((method, params, body))
        if method == 'generateIds':
            return {'ids': [MANIFEST]}
        if method == 'create':
            if self.before_create:
                self.before_create(body, upload)
            if self.loss == 'before':
                self.loss = None
                raise CliError(reason='cli_outcome_unknown')
            raw = Path(upload).read_bytes()
            self.files[body['id']] = (raw, {**body, 'size': str(len(raw)), 'trashed': False,
                                         'capabilities': {'canDownload': True}})
            if self.loss == 'after':
                self.loss = None
                raise CliError(reason='cli_outcome_unknown')
            return {}
        if params['fileId'] == self.folder_id:
            return {'id': self.folder_id, 'mimeType': 'application/vnd.google-apps.folder',
                    'trashed': False, 'capabilities': {'canAddChildren': True}}
        if params['fileId'] not in self.files:
            raise CliError(404)
        raw, metadata = self.files[params['fileId']]
        if output is not None:
            Path(output).write_bytes(b'corrupt' if self.corrupt == params['fileId'] else raw)
            return {}
        return copy.deepcopy(metadata)

    def count(self, method):
        return sum(call[0] == method for call in self.calls)


class ManifestTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        stream = io.BytesIO()
        Image.new('RGB', (2, 3)).save(stream, format='PNG')
        self.raw = stream.getvalue()
        self.original = self.root / 'original.png'
        self.original.write_bytes(self.raw)
        self.sha = hashlib.sha256(self.raw).hexdigest()
        self.sample = {'listing_id': '30', 'company_ids': ['10'],
            'observed_at': '2026-10-05T01:02:03Z', 'source_slot': 1,
            'source_path': str(self.original), 'sha256': self.sha, 'bytes': len(self.raw)}
        self.uploads = [{'sample': 1, 'result': {'file_id': IMAGE, 'sha256': self.sha,
            'original_bytes': len(self.raw), 'readback_hash_matches': True}}]
        self.cli = MockCli(self.raw)
        self.output = self.root / 'receipts'

    def build(self, samples=None):
        return build_manifests({'samples': samples or [self.sample]}, self.uploads, FOLDER)

    def pilot(self):
        return DriveManifestPilot(FOLDER, self.output, self.cli)

    def test_grouping_order_operation_and_previous_chain(self):
        second = {**self.sample, 'source_slot': 2}
        third = {**self.sample, 'listing_id': '31'}
        manifests = self.build([second, third, self.sample])
        self.assertEqual(len(manifests), 2)
        self.assertEqual([x['slot'] for x in manifests[0]['images']], [1, 2])
        self.assertEqual(manifests, self.build([self.sample, second, third]))
        previous = {**self.sample, 'previous_manifest_file_id': MANIFEST,
                    'previous_manifest_sha256': 'a' * 64}
        chained = self.build([previous])[0]
        self.assertEqual(chained['previousManifestSha256'], 'a' * 64)
        self.assertNotEqual(chained['operationId'], self.build()[0]['operationId'])

    def test_source_receipt_slots_and_observation_conflicts_fail_before_network(self):
        invalid = ({**self.sample, 'listing_id': '../30'}, {**self.sample, 'source_slot': True},
            {**self.sample, 'observed_at': '2026-10-05'}, {**self.sample, 'sha256': 'b' * 64},
            {**self.sample, 'bytes': 0}, {**self.sample, 'previous_manifest_file_id': MANIFEST})
        for sample in invalid:
            with self.subTest(sample=list(sample)), self.assertRaises(PilotError):
                self.build([sample])
        with self.assertRaisesRegex(PilotError, 'duplicate_image_slot'):
            self.build([self.sample, self.sample])
        with self.assertRaisesRegex(PilotError, 'listing_observation_conflict'):
            self.build([self.sample, {**self.sample, 'source_slot': 2, 'company_ids': ['11']}])
        self.uploads[0]['result']['readback_hash_matches'] = False
        with self.assertRaisesRegex(PilotError, 'unverified_image_receipt'):
            self.build()
        self.assertEqual(self.cli.calls, [])

    def test_reserved_receipt_precedes_create_repeat_never_recreates(self):
        manifest = self.build()[0]
        def before(body, upload):
            receipt = json.loads((self.output / (manifest['operationId'] + '.json')).read_bytes())
            self.assertEqual((receipt['file_id'], receipt['state']), (MANIFEST, 'sending'))
            self.assertEqual(json.loads(Path(upload).read_bytes()), manifest)
            self.assertEqual(body['appProperties']['job_copy_sync'], 'pending')
        self.cli.before_create = before
        result = self.pilot().ensure_manifest(manifest, True)
        self.assertTrue(result['readback_hash_matches'])
        self.assertEqual(result['sync_state'], 'pending')
        self.assertEqual(result['company_ids'], ['10'])
        self.pilot().ensure_manifest(manifest)
        self.assertEqual((self.cli.count('generateIds'), self.cli.count('create')), (1, 1))

    def test_response_loss_after_success_verifies_same_reserved_file(self):
        self.cli.loss = 'after'
        result = self.pilot().ensure_manifest(self.build()[0], True)
        self.assertEqual(result['manifest_file_id'], MANIFEST)
        self.assertEqual((self.cli.count('generateIds'), self.cli.count('create')), (1, 1))

    def test_response_loss_before_success_restarts_with_same_reserved_id(self):
        self.cli.loss = 'before'
        manifest = self.build()[0]
        with self.assertRaises(CliError):
            self.pilot().ensure_manifest(manifest, True)
        self.pilot().ensure_manifest(manifest, True)
        creates = [call[2]['id'] for call in self.cli.calls if call[0] == 'create']
        self.assertEqual(creates, [MANIFEST, MANIFEST])
        self.assertEqual(self.cli.count('generateIds'), 1)

    def test_wrong_folder_or_image_bytes_prevents_manifest_reservation(self):
        self.cli.files[IMAGE][1]['parents'] = ['foreign_folder_12345']
        with self.assertRaisesRegex(PilotError, 'remote_metadata_mismatch'):
            self.pilot().ensure_manifest(self.build()[0], True)
        self.cli.files[IMAGE][1]['parents'] = [FOLDER]
        self.cli.corrupt = IMAGE
        with self.assertRaisesRegex(PilotError, 'download_hash_mismatch'):
            self.pilot().ensure_manifest(self.build()[0], True)
        self.assertEqual(self.cli.count('generateIds'), 0)

    def test_manifest_readback_corruption_and_mutable_local_copy_are_rejected(self):
        manifest = self.build()[0]
        self.cli.corrupt = MANIFEST
        with self.assertRaisesRegex(PilotError, 'download_hash_mismatch'):
            self.pilot().ensure_manifest(manifest, True)
        self.cli.corrupt = None
        self.pilot().ensure_manifest(manifest)
        (self.output / (manifest['operationId'] + '.manifest.json')).write_bytes(b'changed')
        with self.assertRaisesRegex(PilotError, 'original_evidence_missing_or_changed'):
            self.pilot().ensure_manifest(manifest, True)
        self.assertEqual(self.cli.count('create'), 1)

    def test_first_readonly_run_and_operation_tamper_cannot_create(self):
        manifest = self.build()[0]
        with self.assertRaisesRegex(PilotError, 'upload_flag_required'):
            self.pilot().ensure_manifest(manifest)
        manifest['listingId'] = '31'
        with self.assertRaisesRegex(PilotError, 'manifest_operation_mismatch'):
            self.pilot().ensure_manifest(manifest, True)
        self.assertEqual(self.cli.count('generateIds'), 0)
        self.assertEqual(self.cli.count('create'), 0)

    def test_completed_move_preserves_manifest_bytes_identity_and_create_zero(self):
        manifest = self.build()[0]
        self.pilot().ensure_manifest(manifest, True)
        original = self.cli.files[MANIFEST][0]
        physical = 'synthetic_new_folder_12345'
        self.cli.folder_id = physical
        for _, metadata in self.cli.files.values():
            metadata['parents'] = [physical]
        self.cli.calls.clear()
        rebuilt = build_manifests({'samples': [self.sample]}, self.uploads, physical, identity_folder_id=FOLDER)[0]
        self.assertEqual(rebuilt, manifest)
        result = DriveManifestPilot(physical, self.output, self.cli, identity_folder_id=FOLDER).ensure_manifest(rebuilt, True)
        self.assertEqual(result['manifest_file_id'], MANIFEST)
        self.assertEqual(result['create_attempts'], 0)
        self.assertEqual(self.cli.count('generateIds'), 0)
        self.assertEqual(self.cli.files[MANIFEST][0], original)

    def test_wrong_manifest_identity_and_partial_move_fail_without_create(self):
        manifest = self.build()[0]
        self.pilot().ensure_manifest(manifest, True)
        physical = 'synthetic_new_folder_12345'
        self.cli.folder_id = physical
        self.cli.files[IMAGE][1]['parents'] = [physical]
        self.cli.calls.clear()
        with self.assertRaisesRegex(PilotError, '^manifest_operation_mismatch$'):
            DriveManifestPilot(physical, self.output, self.cli).ensure_manifest(manifest, True)
        with self.assertRaisesRegex(PilotError, '^remote_metadata_mismatch$'):
            DriveManifestPilot(physical, self.output, self.cli, identity_folder_id=FOLDER).ensure_manifest(manifest, True)
        self.cli.files[MANIFEST][1]['parents'] = [physical]
        regenerated_wrong = build_manifests({'samples': [self.sample]}, self.uploads, physical)[0]
        with self.assertRaisesRegex(PilotError, '^receipt_identity_conflict$'):
            DriveManifestPilot(physical, self.output, self.cli).ensure_manifest(regenerated_wrong, True)
        self.assertEqual(self.cli.count('generateIds'), 0)
        self.assertEqual(self.cli.count('create'), 0)


class ReadRetryTests(unittest.TestCase):
    def test_transient_get_retries_are_bounded_and_phase_has_no_identifier(self):
        class Cli:
            calls = 0
            def call(self, *_):
                self.calls += 1
                raise CliError(500)
        cli = Cli()
        sleeps = []
        wrapper = RetryReadCli(cli, sleep=sleeps.append)
        with self.assertRaisesRegex(CliError, '^drive_http_500$'):
            wrapper.call('get', {'fileId': IMAGE, 'alt': 'media'})
        self.assertEqual(cli.calls, 3)
        self.assertEqual(sleeps, [0.25, 0.5])
        self.assertEqual(wrapper.phase, 'get_media')
        self.assertNotIn(IMAGE, wrapper.phase)

    def test_get_recovers_but_create_and_permission_failure_do_not_retry(self):
        class Cli:
            calls = 0
            code = 503
            def call(self, *_):
                self.calls += 1
                if self.calls == 1:
                    raise CliError(self.code)
                return {'ok': True}
        cli = Cli()
        wrapper = RetryReadCli(cli, sleep=lambda _: None)
        self.assertEqual(wrapper.call('get', {'fileId': IMAGE}), {'ok': True})
        self.assertEqual(cli.calls, 2)
        for method, code in [('create', 500), ('generateIds', 500), ('get', 403)]:
            with self.subTest(method=method, code=code):
                cli = Cli()
                cli.code = code
                with self.assertRaises(CliError):
                    RetryReadCli(cli, sleep=lambda _: None).call(method, {})
                self.assertEqual(cli.calls, 1)


class ServiceReadbackTests(unittest.TestCase):
    def test_refresh_transport_failure_is_sanitized_and_retried_only_for_get(self):
        from google.auth.exceptions import TransportError
        class Credentials:
            valid = False
            calls = 0
            def refresh(self, _):
                self.calls += 1
                raise TransportError('synthetic-secret-do-not-log')
        class Session:
            def close(self): pass
        credentials = Credentials()
        readback = ServiceReadbackCli(None, None, credentials, Session())
        wrapper = RetryReadCli(readback, sleep=lambda _: None)
        with self.assertRaisesRegex(CliError, '^readback_credential_transport_failed$'):
            wrapper.call('get', {'fileId': MANIFEST, 'alt': 'media'}, output=Path('unused'))
        self.assertEqual(credentials.calls, 3)
        self.assertEqual(wrapper.transient_read_retries, 2)

    def test_json_media_bytes_are_preserved_without_gws_transformation(self):
        class Credentials:
            valid = True
            token = 'synthetic-test-token'
        class Response:
            status_code = 200
            def __enter__(self): return self
            def __exit__(self, *_): pass
            def iter_content(self, chunk_size): yield b'{"schemaVersion":1}'
        class Session:
            arguments = None
            def get(self, url, **kwargs):
                self.arguments = (url, kwargs)
                return Response()
        class NoGws:
            def call(self, *_): raise AssertionError('media must bypass gws')
        session = Session()
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / 'raw.json'
            cli = ServiceReadbackCli(NoGws(), None, Credentials(), session)
            cli.call('get', {'fileId': MANIFEST, 'alt': 'media'}, output=output)
            self.assertEqual(output.read_bytes(), b'{"schemaVersion":1}')
        self.assertFalse(session.trust_env)
        self.assertEqual(session.arguments[0], 'https://www.googleapis.com/drive/v3/files/' + MANIFEST)
        self.assertFalse(session.arguments[1]['allow_redirects'])

    def test_redirect_and_foreign_file_identifier_are_rejected_without_token_leak(self):
        class Credentials:
            valid = True
            token = 'synthetic-secret'
        class Response:
            status_code = 302
            def __enter__(self): return self
            def __exit__(self, *_): pass
        class Session:
            calls = 0
            def get(self, *_args, **_kwargs):
                self.calls += 1
                return Response()
        session = Session()
        cli = ServiceReadbackCli(None, None, Credentials(), session)
        with self.assertRaisesRegex(CliError, '^drive_http_302$'):
            cli.call('get', {'fileId': MANIFEST, 'alt': 'media'}, output=Path('unused'))
        with self.assertRaisesRegex(PilotError, '^invalid_drive_id$'):
            cli.call('get', {'fileId': '../foreign', 'alt': 'media'}, output=Path('unused'))
        self.assertEqual(session.calls, 1)


if __name__ == '__main__':
    unittest.main()
