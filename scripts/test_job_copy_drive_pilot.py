"""Offline Drive CLI tests. No OAuth lookup or real subprocess/network calls."""
from concurrent.futures import ThreadPoolExecutor
import hashlib
import io
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from PIL import Image

from job_copy_drive_pilot import CliError, DrivePilot, GwsCli, PilotError, MAX_BYTES

FOLDER = 'synthetic_folder_12345'
FILE_ID = 'synthetic_reserved_12345'


class MockCli:
    def __init__(self, raw):
        self.raw = raw
        self.calls = []
        self.metadata = None
        self.loss = None
        self.hide = 0
        self.corrupt = False
        self.folder_valid = True
        self.folder_permission = True
        self.before_create = None
        self.download_error = False
        self.folder_id = FOLDER
        self.folder_parent = 'synthetic_parent_12345'

    def call(self, method, params, body=None, upload=None, output=None):
        self.calls.append((method, params.copy(), body, upload, output))
        if method == 'generateIds':
            return {'ids': [FILE_ID]}
        if method == 'create':
            if self.before_create:
                self.before_create(body, upload)
            if self.loss == 'before':
                self.loss = None
                raise CliError(reason='cli_outcome_unknown')
            if self.metadata is not None:
                raise CliError(409)
            self.metadata = {'id': body['id'], 'mimeType': body['mimeType'], 'size': str(len(self.raw)),
                'parents': body['parents'], 'appProperties': body['appProperties'], 'trashed': False,
                'capabilities': {'canDownload': True}}
            if self.loss == 'after':
                self.loss = None
                raise CliError(reason='cli_outcome_unknown')
            return self.metadata.copy()
        if params['fileId'] == self.folder_id:
            return {'id': self.folder_id, 'parents': [self.folder_parent], 'mimeType': 'application/vnd.google-apps.folder' if self.folder_valid else 'image/png',
                    'trashed': False, 'capabilities': {'canAddChildren': self.folder_permission}}
        if self.metadata is None or self.hide:
            if self.hide:
                self.hide -= 1
            raise CliError(404)
        if output is not None:
            if self.download_error:
                raise CliError(403)
            Path(output).write_bytes(b'corrupt' if self.corrupt else self.raw)
            return {}
        return self.metadata.copy()

    def methods(self):
        return [call[0] for call in self.calls]


class DrivePilotTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.image = self.root / 'original.png'
        buf = io.BytesIO()
        Image.new('RGB', (3, 4), (50, 60, 70)).save(buf, format='PNG')
        self.raw = buf.getvalue()
        self.image.write_bytes(self.raw)
        self.output = self.root / 'private-receipts'
        self.cli = MockCli(self.raw)

    def client(self):
        return DrivePilot(FOLDER, self.output, self.cli)

    def receipt(self):
        files = list(self.output.glob('*.json'))
        self.assertEqual(len(files), 1)
        return json.loads(files[0].read_text(encoding='utf-8'))

    def test_happy_preserves_raw_and_reserved_id_before_create_and_reads_hash(self):
        def before(body, upload):
            receipt = self.receipt()
            self.assertEqual((receipt['file_id'], receipt['state']), (FILE_ID, 'sending'))
            self.assertEqual(Path(upload).read_bytes(), self.raw)
            self.assertEqual(body['appProperties']['job_copy_sha256'], hashlib.sha256(self.raw).hexdigest())
            self.assertEqual(body['parents'], [FOLDER])
        self.cli.before_create = before
        proof = self.client().ensure_image(self.image, True)
        self.assertTrue(proof['readback_hash_matches'])
        self.assertFalse(proof['permissions_changed'])
        self.assertEqual(proof['create_attempts'], 1)
        self.assertEqual(self.receipt()['state'], 'verified')
        self.assertEqual(self.cli.methods().count('generateIds'), 1)
        self.assertEqual(self.cli.methods().count('create'), 1)
        self.assertEqual([call[1].get('alt') for call in self.cli.calls if call[4]], ['media'])

    def test_repeat_reuses_id_without_generate_or_create(self):
        self.client().ensure_image(self.image, True)
        self.cli.calls.clear()
        proof = self.client().ensure_image(self.image)
        self.assertEqual(proof['file_id'], FILE_ID)
        self.assertEqual(proof['create_attempts'], 0)
        self.assertNotIn('generateIds', self.cli.methods())
        self.assertNotIn('create', self.cli.methods())

    def test_create_success_response_lost_recovers_same_id(self):
        self.cli.loss = 'after'
        proof = self.client().ensure_image(self.image, True)
        self.assertTrue(proof['readback_hash_matches'])
        self.assertEqual(proof['file_id'], FILE_ID)
        self.assertEqual(self.cli.methods().count('create'), 1)
        self.assertEqual(self.cli.methods().count('generateIds'), 1)

    def test_lost_create_with_no_file_retries_only_persisted_id(self):
        self.cli.loss = 'before'
        with self.assertRaisesRegex(CliError, '^cli_outcome_unknown$'):
            self.client().ensure_image(self.image, True)
        self.assertEqual(self.receipt()['state'], 'uncertain')
        self.assertEqual(self.receipt()['file_id'], FILE_ID)
        proof = self.client().ensure_image(self.image, True)
        self.assertTrue(proof['readback_hash_matches'])
        self.assertEqual(self.cli.methods().count('generateIds'), 1)
        creates = [call[2]['id'] for call in self.cli.calls if call[0] == 'create']
        self.assertEqual(creates, [FILE_ID, FILE_ID])

    def test_409_after_delayed_lookup_recovers_existing_id(self):
        self.client().ensure_image(self.image, True)
        self.cli.hide = 1
        self.cli.calls.clear()
        proof = self.client().ensure_image(self.image, True)
        self.assertEqual(proof['file_id'], FILE_ID)
        self.assertEqual(self.cli.methods().count('create'), 1)
        self.assertNotIn('generateIds', self.cli.methods())

    def test_parallel_identical_operations_generate_and_create_once(self):
        def run(_):
            return self.client().ensure_image(self.image, True)['file_id']
        with ThreadPoolExecutor(max_workers=4) as pool:
            self.assertEqual(list(pool.map(run, range(4))), [FILE_ID] * 4)
        self.assertEqual(self.cli.methods().count('generateIds'), 1)
        self.assertEqual(self.cli.methods().count('create'), 1)

    def test_hash_mismatch_remains_failed_and_never_recreates(self):
        self.cli.corrupt = True
        with self.assertRaisesRegex(PilotError, '^download_hash_mismatch$'):
            self.client().ensure_image(self.image, True)
        self.assertEqual(self.receipt()['state'], 'verification_failed')
        self.assertEqual(list(self.output.glob('*.download')), [])
        self.cli.corrupt = False
        proof = self.client().ensure_image(self.image)
        self.assertTrue(proof['readback_hash_matches'])
        self.assertEqual(self.cli.methods().count('create'), 1)

    def test_download_permission_failure_is_not_verified(self):
        self.cli.download_error = True
        with self.assertRaisesRegex(CliError, '^drive_http_403$'):
            self.client().ensure_image(self.image, True)
        self.assertEqual(self.receipt()['state'], 'uploaded_unverified')
        self.cli.download_error = False
        self.client().ensure_image(self.image)
        self.assertEqual(self.cli.methods().count('create'), 1)

    def test_folder_mime_or_capability_failure_prevents_id_reservation(self):
        self.cli.folder_valid = False
        with self.assertRaisesRegex(PilotError, '^invalid_destination_folder$'):
            self.client().ensure_image(self.image, True)
        self.cli.folder_valid = True
        self.cli.folder_permission = False
        with self.assertRaisesRegex(PilotError, '^folder_add_permission_missing$'):
            self.client().ensure_image(self.image, True)
        self.assertNotIn('generateIds', self.cli.methods())

    def test_read_only_first_run_never_reserves_or_creates(self):
        with self.assertRaisesRegex(PilotError, '^upload_flag_required$'):
            self.client().ensure_image(self.image)
        self.assertEqual(self.cli.methods(), ['get'])

    def test_metadata_mismatch_blocks_readback_and_recreate(self):
        self.client().ensure_image(self.image, True)
        for field, value in [('parents', ['another_folder_12345']), ('mimeType', 'image/jpeg'), ('size', '0'), ('trashed', True), ('appProperties', {})]:
            with self.subTest(field=field):
                original = self.cli.metadata[field]
                self.cli.metadata[field] = value
                self.cli.calls.clear()
                with self.assertRaisesRegex(PilotError, '^remote_metadata_mismatch$'):
                    self.client().ensure_image(self.image, True)
                self.assertNotIn('create', self.cli.methods())
                self.assertFalse(any(call[4] for call in self.cli.calls))
                self.cli.metadata[field] = original

    def test_missing_or_changed_local_original_is_not_silently_replaced(self):
        self.client().ensure_image(self.image, True)
        original = self.output / (hashlib.sha256(self.raw).hexdigest() + '.png')
        original.write_bytes(b'changed-local-original')
        with self.assertRaisesRegex(PilotError, '^original_evidence_missing_or_changed$'):
            self.client().ensure_image(self.image, True)
        self.assertEqual(self.cli.methods().count('create'), 1)

    def test_invalid_id_image_or_large_file_makes_no_cli_call(self):
        with self.assertRaisesRegex(PilotError, '^invalid_drive_id$'):
            DrivePilot('../folder', self.output, self.cli)
        self.image.write_bytes(b'not-an-image')
        with self.assertRaisesRegex(PilotError, '^invalid_image$'):
            self.client().ensure_image(self.image, True)
        self.image.write_bytes(b'x' * (MAX_BYTES + 1))
        with self.assertRaisesRegex(PilotError, '^image_too_large$'):
            self.client().ensure_image(self.image, True)
        self.assertEqual(self.cli.calls, [])

    def test_completed_move_preserves_image_identity_receipt_and_file_id(self):
        first = self.client().ensure_image(self.image, True)
        receipt = self.receipt()
        physical = 'synthetic_new_folder_12345'
        self.cli.folder_id = physical
        self.cli.metadata['parents'] = [physical]
        self.cli.calls.clear()
        result = DrivePilot(physical, self.output, self.cli, identity_folder_id=FOLDER).ensure_image(self.image, True)
        self.assertEqual(result['file_id'], first['file_id'])
        self.assertEqual(result['create_attempts'], 0)
        self.assertNotIn('generateIds', self.cli.methods())
        self.assertEqual(self.receipt()['folder_id'], receipt['folder_id'])

    def test_wrong_identity_after_move_fails_before_reservation_even_with_upload(self):
        self.client().ensure_image(self.image, True)
        physical = 'synthetic_new_folder_12345'
        self.cli.folder_id = physical
        self.cli.metadata['parents'] = [physical]
        self.cli.calls.clear()
        with self.assertRaisesRegex(PilotError, '^receipt_identity_conflict$'):
            DrivePilot(physical, self.output, self.cli).ensure_image(self.image, True)
        self.assertNotIn('generateIds', self.cli.methods())
        self.assertNotIn('create', self.cli.methods())

    def test_partial_move_fails_and_folder_parent_change_preserves_same_identity(self):
        self.client().ensure_image(self.image, True)
        self.cli.folder_parent = 'synthetic_new_parent_12345'
        self.cli.calls.clear()
        self.client().ensure_image(self.image, True)
        self.assertNotIn('create', self.cli.methods())
        physical = 'synthetic_new_folder_12345'
        self.cli.folder_id = physical  # Original still has old parent.
        with self.assertRaisesRegex(PilotError, '^remote_metadata_mismatch$'):
            DrivePilot(physical, self.output, self.cli, identity_folder_id=FOLDER).ensure_image(self.image, True)
        self.assertNotIn('create', self.cli.methods())


class GwsCliTests(unittest.TestCase):
    def test_argv_uses_utf8_json_upload_and_output_without_shell(self):
        cli = GwsCli('synthetic-gws.cmd')
        with patch('job_copy_drive_pilot.subprocess.run', return_value=subprocess.CompletedProcess([], 0, '{}', '')) as run:
            cli.call('create', {'supportsAllDrives': True}, body={'id': FILE_ID, 'name': '架空画像'}, upload=Path('synthetic.png'))
            argv = run.call_args.args[0]
            self.assertEqual(json.loads(argv[argv.index('--json') + 1])['name'], '架空画像')
            self.assertIn('--upload', argv)
            self.assertEqual(run.call_args.kwargs['encoding'], 'utf-8')
            self.assertNotIn('shell', run.call_args.kwargs)
            cli.call('get', {'fileId': FILE_ID, 'alt': 'media'}, output=Path('synthetic.download'))
            self.assertIn('--output', run.call_args.args[0])

    def test_cli_errors_do_not_expose_upstream_messages_or_tokens(self):
        cli = GwsCli('synthetic-gws.cmd')
        error = subprocess.CompletedProcess([], 1, json.dumps({'error': {'code': 404, 'message': 'synthetic-token-secret'}}), '')
        with patch('job_copy_drive_pilot.subprocess.run', return_value=error), self.assertRaisesRegex(CliError, '^drive_http_404$'):
            cli.call('get', {'fileId': FILE_ID})
        with patch('job_copy_drive_pilot.subprocess.run', side_effect=subprocess.TimeoutExpired('secret-argv', 120)), self.assertRaisesRegex(CliError, '^cli_outcome_unknown$'):
            cli.call('get', {'fileId': FILE_ID})

    def test_permissions_update_delete_commands_are_rejected(self):
        cli = GwsCli('synthetic-gws.cmd')
        with patch('job_copy_drive_pilot.subprocess.run') as run:
            for method in ['update', 'delete', 'permissions.create', 'copy']:
                with self.subTest(method=method), self.assertRaisesRegex(PilotError, '^disallowed_drive_command$'):
                    cli.call(method, {})
            run.assert_not_called()


if __name__ == '__main__':
    unittest.main()
