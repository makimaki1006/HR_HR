"""Archive a bounded, complete review image batch in the configured Drive folder.

Uses existing durable reservation receipts and raw-byte readback. This command
does not write HubSpot, alter sharing, move files, or schedule future runs.
"""
import argparse
from datetime import datetime, timezone
import json
from pathlib import Path
import uuid

from job_copy_drive_pilot import DrivePilot, GwsCli, PilotError, identifier
from job_copy_drive_manifest import DriveManifestPilot, RetryReadCli, ServiceReadbackCli, build_manifests
from job_copy_snapshot_store import LocalSnapshotStore, canonical


def preflight(samples, folder, identity):
    rows = samples.get('samples') if isinstance(samples, dict) else None
    if not isinstance(rows, list) or not 1 <= len(rows) <= 5900:
        raise PilotError('invalid_samples')
    unique = {}
    for row in rows:
        if not isinstance(row, dict):
            raise PilotError('invalid_sample')
        digest = row.get('sha256')
        if not isinstance(digest, str):
            raise PilotError('invalid_image_hash')
        unique.setdefault(digest, row)
    # Schema/raw-original validation only. These placeholder IDs remain in
    # memory and are never persisted, uploaded, or treated as Drive evidence.
    placeholders = [{'result': {'file_id': 'preflight_' + digest, 'sha256': digest,
                               'original_bytes': row.get('bytes'), 'readback_hash_matches': True}}
                    for digest, row in unique.items()]
    manifests = build_manifests(samples, placeholders, folder, identity_folder_id=identity)
    if not 1 <= len(manifests) <= 59:
        raise PilotError('batch_job_limit')
    return list(unique.values()), len(manifests)


def archive(samples, config, key, output, upload=False):
    if not isinstance(config, dict) or config.get('schemaVersion') != 1:
        raise PilotError('invalid_storage_config')
    folder = identifier(config.get('folderId'))
    identity = identifier(config.get('identityFolderId'))
    unique, expected_manifests = preflight(samples, folder, identity)
    output = Path(output).resolve()
    output.mkdir(parents=True, exist_ok=True)
    state = LocalSnapshotStore(output / '.batch-lock')
    transport = ServiceReadbackCli(GwsCli(), key)
    image_pilot = DrivePilot(folder, config['imageReceiptDir'], RetryReadCli(transport), identity_folder_id=identity)
    manifest_pilot = DriveManifestPilot(folder, config['manifestReceiptDir'], transport, identity_folder_id=identity)
    uploads, manifests = [], []
    phase = 'images'
    with state._lock():
        attempt = {'runId': uuid.uuid4().hex,
                   'startedAt': datetime.now(timezone.utc).isoformat()}
        try:
            state._atomic(output / 'summary.json', canonical({**attempt, 'state': 'running', 'ok': False}))
            for index, sample in enumerate(unique):
                result = image_pilot.ensure_image(sample['source_path'], upload=upload)
                uploads.append({'sample': index + 1, 'result': result})
                state._atomic(output / 'upload-results.json', canonical(uploads))
                print(json.dumps({'phase': phase, 'verified': len(uploads), 'total': len(unique),
                                  'create_attempts': image_pilot.create_attempts}), flush=True)
            phase = 'manifests'
            pending = build_manifests(samples, uploads, folder, identity_folder_id=identity)
            for manifest in pending:
                manifests.append(manifest_pilot.ensure_manifest(manifest, upload=upload))
                state._atomic(output / 'manifest-results.json', canonical({'results': manifests}))
                print(json.dumps({'phase': phase, 'verified': len(manifests), 'total': expected_manifests,
                                  'create_attempts': manifest_pilot.create_attempts}), flush=True)
            summary = {**attempt, 'state': 'succeeded', 'ok': True,
                       'unique_originals': len(uploads), 'image_references': len(samples['samples']),
                       'original_bytes': sum(row['result']['original_bytes'] for row in uploads),
                       'manifests': len(manifests), 'image_create_attempts': image_pilot.create_attempts,
                       'manifest_create_attempts': manifest_pilot.create_attempts,
                       'hubspot_writes': 0, 'files_moved': 0, 'permissions_changed': False}
            state._atomic(output / 'summary.json', canonical(summary))
            (output / 'last-failure.json').unlink(missing_ok=True)
            return summary
        except Exception as error:
            code = str(error) if isinstance(error, PilotError) else type(error).__name__
            failure = {**attempt, 'state': 'failed', 'ok': False, 'phase': phase, 'error': code,
                       'verified_originals': len(uploads), 'verified_manifests': len(manifests)}
            state._atomic(output / 'summary.json', canonical(failure))
            state._atomic(output / 'last-failure.json', canonical(failure))
            raise


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    for flag in ('samples', 'storage-config', 'service-key', 'output'):
        parser.add_argument('--' + flag, type=Path, required=True)
    parser.add_argument('--upload', action='store_true')
    args = parser.parse_args()
    try:
        summary = archive(json.loads(args.samples.read_text(encoding='utf-8')),
                          json.loads(args.storage_config.read_text(encoding='utf-8')),
                          args.service_key, args.output, args.upload)
        print(json.dumps(summary), flush=True)
    except Exception as error:
        print(json.dumps({'ok': False, 'error': str(error) if isinstance(error, PilotError) else type(error).__name__}), flush=True)
        raise SystemExit(1) from None
