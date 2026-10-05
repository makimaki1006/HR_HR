"""Explicit-folder image pilot using existing gws OAuth; not production storage.

No permissions, updates, deletes, HubSpot writes, or credential export. Original
bytes and a reserved Drive ID are retained in --receipt-dir before CREATE. Keep
that directory private and durable locally: it is not a production Retry store.
Reuse the SAME receipt directory for the same operation. Different directories
cannot share reservation receipts and may create separate files for equal bytes.
Without --upload, only an existing receipt/file can be read and verified.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import io
import json
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

from PIL import Image

from job_copy_snapshot_store import LocalSnapshotStore, canonical

MAX_BYTES = 5 * 1024 * 1024
FILE_FIELDS = 'id,mimeType,size,parents,trashed,appProperties,capabilities(canDownload)'


class PilotError(RuntimeError):
    pass


class CliError(PilotError):
    def __init__(self, code=None, reason='cli_failed'):
        self.code = code
        super().__init__(f'drive_http_{code}' if code is not None else reason)


def identifier(value):
    if not isinstance(value, str) or not re.fullmatch(r'[A-Za-z0-9_-]{10,200}', value):
        raise PilotError('invalid_drive_id')
    return value


def evidence(path):
    path = Path(path)
    if path.stat().st_size > MAX_BYTES:
        raise PilotError('image_too_large')
    raw = path.read_bytes()
    if not raw or len(raw) > MAX_BYTES:
        raise PilotError('image_too_large_or_empty')
    try:
        with Image.open(io.BytesIO(raw)) as image:
            formats = {'JPEG': ('image/jpeg', 'jpg'), 'PNG': ('image/png', 'png'), 'WEBP': ('image/webp', 'webp')}
            if image.format not in formats or image.width * image.height > 30_000_000:
                raise PilotError('invalid_image')
            mime, extension = formats[image.format]
            image.verify()
    except PilotError:
        raise
    except Exception:
        raise PilotError('invalid_image') from None
    return raw, mime, extension, hashlib.sha256(raw).hexdigest()


class GwsCli:
    """Only a small set of Drive commands, with sanitized failures."""
    def __init__(self, executable=None):
        self.executable = executable or shutil.which('gws.cmd') or shutil.which('gws')
        if not self.executable:
            raise PilotError('gws_missing')

    def call(self, method, params, body=None, upload=None, output=None):
        if method not in ('get', 'generateIds', 'create'):
            raise PilotError('disallowed_drive_command')
        if method == 'create':
            if not isinstance(body, dict) or upload is None or output is not None:
                raise PilotError('invalid_create_arguments')
            identifier(body.get('id'))
        elif body is not None or upload is not None:
            raise PilotError('disallowed_drive_arguments')
        if method == 'get':
            identifier(params.get('fileId'))
        if output is not None and (method != 'get' or params.get('alt') != 'media'):
            raise PilotError('invalid_download_arguments')
        args = [self.executable, 'drive', 'files', method, '--params', json.dumps(params)]
        if body is not None:
            args += ['--json', json.dumps(body)]
        if upload is not None:
            args += ['--upload', str(upload)]
        if output is not None:
            # Actual gws help names this --output / -o, not --output-file.
            args += ['--output', str(output)]
        try:
            result = subprocess.run(args, capture_output=True, encoding='utf-8', errors='replace', timeout=120, check=False)
        except subprocess.TimeoutExpired:
            raise CliError(reason='cli_outcome_unknown') from None
        except OSError:
            raise CliError(reason='cli_launch_failed') from None
        try:
            data = json.loads(result.stdout) if result.stdout.strip() else {}
        except (ValueError, TypeError):
            data = None
        if result.returncode != 0 or isinstance(data, dict) and 'error' in data:
            code = None
            for content in (data, self._json(result.stderr)):
                error = content.get('error') if isinstance(content, dict) else None
                candidate = error.get('code') if isinstance(error, dict) else None
                if isinstance(candidate, int) and 100 <= candidate <= 599:
                    code = candidate
                    break
            raise CliError(code=code)
        if output is not None:
            return {}
        if not isinstance(data, dict):
            raise CliError(reason='invalid_cli_response')
        return data

    @staticmethod
    def _json(content):
        try:
            return json.loads(content)
        except (TypeError, ValueError):
            return None


class DrivePilot:
    def __init__(self, folder_id, receipt_dir, cli=None, *, identity_folder_id=None):
        self.folder_id = identifier(folder_id)
        # Stable operation namespace; actual metadata/destination always uses folder_id.
        self.identity_folder_id = identifier(identity_folder_id) if identity_folder_id is not None else self.folder_id
        self.output = Path(receipt_dir).resolve()
        self.store = LocalSnapshotStore(self.output / 'local-lock')
        self.cli = cli or GwsCli()
        self.create_attempts = 0

    def folder(self):
        folder = self.cli.call('get', {'fileId': self.folder_id, 'supportsAllDrives': True,
            'fields': 'id,mimeType,trashed,capabilities(canAddChildren)'})
        if folder.get('id') != self.folder_id or folder.get('mimeType') != 'application/vnd.google-apps.folder' or folder.get('trashed') is not False:
            raise PilotError('invalid_destination_folder')
        if folder.get('capabilities', {}).get('canAddChildren') is not True:
            raise PilotError('folder_add_permission_missing')

    def metadata(self, file_id):
        try:
            return self.cli.call('get', {'fileId': identifier(file_id), 'supportsAllDrives': True, 'fields': FILE_FIELDS})
        except CliError as error:
            if error.code == 404:
                return None
            raise

    def ensure_image(self, image, upload=False):
        raw, mime, extension, digest = evidence(image)
        operation = hashlib.sha256(canonical({'folder_id': self.identity_folder_id, 'sha256': digest})).hexdigest()
        receipt_path = self.output / f'{operation}.json'
        original_path = self.output / f'{digest}.{extension}'
        with self.store._lock():
            self.folder()
            receipt = json.loads(receipt_path.read_text(encoding='utf-8')) if receipt_path.exists() else None
            # folder_id in legacy receipts is the namespace, not current placement.
            expected = {'folder_id': self.identity_folder_id, 'sha256': digest, 'size': len(raw), 'mime_type': mime}
            if receipt is not None and any(receipt.get(key) != value for key, value in expected.items()):
                raise PilotError('receipt_conflict')
            if receipt is None:
                for prior_path in self.output.glob('*.json'):
                    if not re.fullmatch(r'[a-f0-9]{64}\.json', prior_path.name):
                        continue
                    prior = json.loads(prior_path.read_text(encoding='utf-8'))
                    if (isinstance(prior, dict) and prior.get('sha256') == digest
                            and prior.get('mime_type') == mime and prior.get('size') == len(raw)
                            and prior.get('folder_id') != self.identity_folder_id):
                        raise PilotError('receipt_identity_conflict')
                if not upload:
                    raise PilotError('upload_flag_required')
                self.store._immutable(original_path, raw)
                generated = self.cli.call('generateIds', {'count': 1, 'space': 'drive', 'type': 'files'})
                ids = generated.get('ids')
                if not isinstance(ids, list) or len(ids) != 1:
                    raise PilotError('invalid_reserved_id_response')
                file_id = identifier(ids[0])
                receipt = {**expected, 'file_id': file_id, 'state': 'reserved', 'classification': 'local_pilot_only'}
                self.store._atomic(receipt_path, canonical(receipt))
            else:
                file_id = identifier(receipt.get('file_id'))
                if not original_path.is_file() or original_path.read_bytes() != raw:
                    raise PilotError('original_evidence_missing_or_changed')
            metadata = self.metadata(file_id)
            if metadata is None:
                if not upload:
                    raise PilotError('reserved_file_not_found_upload_flag_required')
                receipt['state'] = 'sending'
                self.store._atomic(receipt_path, canonical(receipt))
                self.create_attempts += 1
                try:
                    # Use the persisted immutable bytes, never a mutable caller file.
                    self.cli.call('create', {'supportsAllDrives': True, 'fields': FILE_FIELDS}, body={
                        'id': file_id, 'name': f'job-copy-{digest}.{extension}', 'mimeType': mime,
                        'parents': [self.folder_id], 'appProperties': {'job_copy_sha256': digest}}, upload=original_path)
                except PilotError as error:
                    receipt.update(state='uncertain', last_error=str(error))
                    self.store._atomic(receipt_path, canonical(receipt))
                    # Response loss or ID-conflict may follow a successful CREATE.
                    metadata = self.metadata(file_id)
                    if metadata is None:
                        raise error
                else:
                    metadata = self.metadata(file_id)
                    if metadata is None:
                        receipt['state'] = 'uncertain'
                        self.store._atomic(receipt_path, canonical(receipt))
                        raise PilotError('created_file_not_yet_visible')
            valid = (metadata.get('id') == file_id and metadata.get('mimeType') == mime
                and str(metadata.get('size')) == str(len(raw)) and metadata.get('parents') == [self.folder_id]
                and metadata.get('trashed') is False and metadata.get('appProperties', {}).get('job_copy_sha256') == digest
                and metadata.get('capabilities', {}).get('canDownload') is True)
            if not valid:
                receipt['state'] = 'verification_failed'
                self.store._atomic(receipt_path, canonical(receipt))
                raise PilotError('remote_metadata_mismatch')
            receipt['state'] = 'uploaded_unverified'
            self.store._atomic(receipt_path, canonical(receipt))
            temporary = None
            try:
                with tempfile.NamedTemporaryFile(dir=self.output, suffix='.download', delete=False) as handle:
                    temporary = Path(handle.name)
                self.cli.call('get', {'fileId': file_id, 'alt': 'media', 'supportsAllDrives': True}, output=temporary)
                if temporary.stat().st_size != len(raw) or hashlib.sha256(temporary.read_bytes()).hexdigest() != digest:
                    receipt['state'] = 'verification_failed'
                    self.store._atomic(receipt_path, canonical(receipt))
                    raise PilotError('download_hash_mismatch')
            finally:
                if temporary is not None and temporary.exists():
                    temporary.unlink()
            receipt.update(state='verified', verified_at=datetime.now(timezone.utc).isoformat())
            receipt.pop('last_error', None)
            self.store._atomic(receipt_path, canonical(receipt))
            return {'classification': 'local_pilot_only', 'file_id': file_id, 'sha256': digest,
                    'original_bytes': len(raw), 'readback_hash_matches': True,
                    'create_attempts': self.create_attempts, 'permissions_changed': False}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--folder-id', required=True)
    parser.add_argument('--identity-folder-id', help='Stable initial operation namespace; defaults to folder-id')
    parser.add_argument('--image', type=Path, required=True)
    parser.add_argument('--receipt-dir', type=Path, required=True)
    parser.add_argument('--upload', action='store_true', help='Permit CREATE with the persisted reserved ID')
    args = parser.parse_args()
    try:
        print(json.dumps(DrivePilot(args.folder_id, args.receipt_dir,
            identity_folder_id=args.identity_folder_id).ensure_image(args.image, args.upload)))
    except Exception as error:
        # No CLI stdout/stderr, source paths, OAuth tokens or upstream messages.
        reason = str(error) if isinstance(error, PilotError) else type(error).__name__
        print(json.dumps({'ok': False, 'error': reason}))
        raise SystemExit(1)


if __name__ == '__main__':
    main()
