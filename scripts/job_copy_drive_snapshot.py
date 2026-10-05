"""Operator-only immutable review snapshot upload; no HubSpot or permission writes.

The snapshot is captured review data, not a new CRM master or daily sync. Reuse
the same private receipt directory and stable identityFolderId for retries.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re

from job_copy_drive_pilot import DrivePilot, GwsCli, PilotError, identifier, FILE_FIELDS
from job_copy_drive_manifest import DriveManifestPilot, RetryReadCli, ServiceReadbackCli
from job_copy_snapshot_store import canonical

SNAPSHOT_LIMIT = 32 * 1024 * 1024


def snapshot_bytes(path):
    try:
        with Path(path).open('rb') as source:
            raw = source.read(SNAPSHOT_LIMIT + 1)
        if not raw or len(raw) > SNAPSHOT_LIMIT:
            raise PilotError('snapshot_size_invalid')
        if raw.startswith(b'\xef\xbb\xbf'):
            raise PilotError('snapshot_utf8_bom_unsupported')
        def invalid_constant(_):
            raise ValueError()
        value = json.loads(raw.decode('utf-8'), parse_constant=invalid_constant)
    except PilotError:
        raise
    except (OSError, ValueError, TypeError):
        raise PilotError('snapshot_unavailable_or_invalid') from None
    if (not isinstance(value, dict) or type(value.get('schemaVersion')) is not int
            or value['schemaVersion'] != 1 or not isinstance(value.get('capture_bundle'), dict)
            or not isinstance(value['capture_bundle'].get('jobs'), list)
            or not isinstance(value.get('results'), list)):
        raise PilotError('snapshot_schema_invalid')
    return raw


class DriveSnapshotPilot(DrivePilot):
    def __init__(self, config, receipt_dir, cli):
        if not isinstance(config, dict) or type(config.get('schemaVersion')) is not int or config['schemaVersion'] != 1:
            raise PilotError('invalid_storage_config')
        super().__init__(identifier(config.get('folderId')), receipt_dir, RetryReadCli(cli),
                         identity_folder_id=identifier(config.get('identityFolderId')))

    def ensure_snapshot(self, source, upload=False):
        raw = snapshot_bytes(source)
        digest = hashlib.sha256(raw).hexdigest()
        operation = hashlib.sha256(canonical({'identityFolderId': self.identity_folder_id,
                            'sha256': digest, 'kind': 'review_snapshot'})).hexdigest()
        receipt_path = self.output / (operation + '.json')
        original_path = self.output / (digest + '.review.json')
        properties = {'job_copy_kind': 'review_snapshot', 'job_copy_sha256': digest,
                      'job_copy_operation': operation}
        expected = {'folder_id': self.identity_folder_id, 'sha256': digest, 'size': len(raw),
                    'operation_id': operation, 'kind': 'review_snapshot'}
        with self.store._lock():
            self.folder()
            receipt = json.loads(receipt_path.read_text(encoding='utf-8')) if receipt_path.exists() else None
            if receipt is not None and any(receipt.get(key) != value for key, value in expected.items()):
                raise PilotError('receipt_conflict')
            if receipt is None:
                # An accidental identity change must not reserve a duplicate in
                # this existing receipt directory after a physical folder move.
                for prior_path in self.output.glob('*.json'):
                    if not re.fullmatch(r'[a-f0-9]{64}\.json', prior_path.name):
                        continue
                    prior = json.loads(prior_path.read_text(encoding='utf-8'))
                    if (isinstance(prior, dict) and prior.get('kind') == 'review_snapshot'
                            and prior.get('sha256') == digest and prior.get('size') == len(raw)
                            and prior.get('folder_id') != self.identity_folder_id):
                        raise PilotError('receipt_identity_conflict')
                if not upload:
                    raise PilotError('upload_flag_required')
                self.store._immutable(original_path, raw)
                ids = self.cli.call('generateIds', {'count': 1, 'space': 'drive', 'type': 'files'}).get('ids')
                if not isinstance(ids, list) or len(ids) != 1:
                    raise PilotError('invalid_reserved_id_response')
                file_id = identifier(ids[0])
                receipt = {**expected, 'file_id': file_id, 'state': 'reserved'}
                self.store._atomic(receipt_path, canonical(receipt))
            else:
                file_id = identifier(receipt.get('file_id'))
                if not original_path.is_file() or original_path.read_bytes() != raw:
                    raise PilotError('original_evidence_missing_or_changed')
            if self.metadata(file_id) is None:
                if not upload:
                    raise PilotError('reserved_file_not_found_upload_flag_required')
                receipt['state'] = 'sending'
                self.store._atomic(receipt_path, canonical(receipt))
                self.create_attempts += 1
                try:
                    self.cli.call('create', {'supportsAllDrives': True, 'fields': FILE_FIELDS}, body={
                        'id': file_id, 'name': f'job-copy-review-{digest}.json', 'mimeType': 'application/json',
                        'parents': [self.folder_id], 'appProperties': properties}, upload=original_path)
                except PilotError as error:
                    receipt.update(state='uncertain', last_error=str(error))
                    self.store._atomic(receipt_path, canonical(receipt))
                    if self.metadata(file_id) is None:
                        raise
            receipt['state'] = 'uploaded_unverified'
            self.store._atomic(receipt_path, canonical(receipt))
            try:
                metadata = DriveManifestPilot.verify_remote(self, file_id, 'application/json', len(raw), digest, properties)
                if metadata is None:
                    raise PilotError('created_file_not_yet_visible')
            except PilotError:
                receipt['state'] = 'verification_failed'
                self.store._atomic(receipt_path, canonical(receipt))
                raise
            receipt.update(state='verified')
            receipt.pop('last_error', None)
            self.store._atomic(receipt_path, canonical(receipt))
            return {'id': file_id, 'sha256': digest, 'bytes': len(raw), 'readback_hash_matches': True}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for flag in ('input', 'storage-config', 'service-key', 'receipt-dir', 'output'):
        parser.add_argument('--' + flag, type=Path, required=True)
    parser.add_argument('--upload', action='store_true')
    args = parser.parse_args()
    try:
        config = json.loads(args.storage_config.read_text(encoding='utf-8-sig'))
        cli = ServiceReadbackCli(GwsCli(), args.service_key, max_bytes=SNAPSHOT_LIMIT)
        pilot = DriveSnapshotPilot(config, args.receipt_dir, cli)
        result = pilot.ensure_snapshot(args.input, args.upload)
        args.output.parent.mkdir(parents=True, exist_ok=True)
        pilot.store._atomic(args.output, canonical(result))
        print(json.dumps(result))
    except Exception as error:
        print(json.dumps({'ok': False, 'error': str(error) if isinstance(error, PilotError) else type(error).__name__}))
        raise SystemExit(1) from None


if __name__ == '__main__':
    main()
