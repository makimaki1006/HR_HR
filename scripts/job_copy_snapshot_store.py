"""Local validation store; NOT production persistence or HubSpot synchronization.

Immutable raw blobs and observations; durable local Pending operations. No network,
CRM database, media download, or HubSpot writes. The operation is snapshot commit
itself; replay safety must not be generalized to an external CREATE API.
"""
from __future__ import annotations

import base64
from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path
import tempfile


DEFAULT_ROOT = Path(__file__).resolve().parents[1] / "data/job-copy-local/storage-validation"


def canonical(value: dict) -> bytes:
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"),
                      allow_nan=False).encode("utf-8")


def digest(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


class Conflict(ValueError):
    """An existing observation identity was reused with different evidence."""


class LocalSnapshotStore:
    def __init__(self, root: Path = DEFAULT_ROOT):
        self.root = Path(root).resolve()
        self.root.mkdir(parents=True, exist_ok=True)
        for name in ("blobs", "observations", "operations"):
            (self.root / name).mkdir(exist_ok=True)

    @contextmanager
    def _lock(self):
        # OS lock is released after process death; no stale PID lock deletion.
        with (self.root / ".store.lock").open("a+b") as handle:
            if os.name == "nt":
                import msvcrt
                if handle.seek(0, os.SEEK_END) == 0:
                    handle.write(b"0")
                    handle.flush()
                handle.seek(0)
                msvcrt.locking(handle.fileno(), msvcrt.LK_LOCK, 1)
            else:
                import fcntl
                fcntl.flock(handle, fcntl.LOCK_EX)
            try:
                yield
            finally:
                if os.name == "nt":
                    handle.seek(0)
                    msvcrt.locking(handle.fileno(), msvcrt.LK_UNLCK, 1)
                else:
                    fcntl.flock(handle, fcntl.LOCK_UN)

    def _atomic(self, path: Path, raw: bytes):
        temporary = None
        try:
            with tempfile.NamedTemporaryFile(dir=path.parent, delete=False) as handle:
                temporary = Path(handle.name)
                handle.write(raw)
                handle.flush()
                os.fsync(handle.fileno())
            os.replace(temporary, path)
            if os.name != "nt":
                descriptor = os.open(path.parent, os.O_RDONLY)
                try:
                    os.fsync(descriptor)
                finally:
                    os.close(descriptor)
        finally:
            if temporary and temporary.exists():
                temporary.unlink()

    def _immutable(self, path: Path, raw: bytes):
        if path.exists():
            if path.read_bytes() != raw:
                raise Conflict("Immutable evidence differs")
            return
        self._atomic(path, raw)

    @staticmethod
    def _identifier(identifier: str):
        if len(identifier) != 64 or any(c not in "0123456789abcdef" for c in identifier):
            raise ValueError("Expected SHA-256 identifier")

    def enqueue(self, identity: dict, metadata: dict, raw_source: bytes,
                images: list[dict] | None = None) -> str:
        """Identity includes stable batch/job keys; retry must reuse that identity.

        Images contain slot, bytes, acquired_at and provenance. Failed images
        belong in metadata, never as empty successful blobs. Later acquisition
        is separate evidence, not a mutation of the original observation.
        """
        required = ("media", "account_key", "media_job_id", "batch_id", "observed_at")
        if any(not isinstance(identity.get(key), str) or not identity[key] for key in required):
            raise ValueError("Missing stable observation identity")
        if not isinstance(raw_source, bytes) or not raw_source:
            raise ValueError("Raw source bytes are required")
        if not isinstance(metadata, dict):
            raise ValueError("Metadata must be an object")
        identity = json.loads(canonical(identity))
        observation_id = digest(canonical(identity))
        blobs = {digest(raw_source): base64.b64encode(raw_source).decode("ascii")}
        image_manifest = []
        seen_slots = set()
        for image in images or []:
            slot, raw = image.get("slot"), image.get("bytes")
            if (not isinstance(slot, int) or isinstance(slot, bool) or slot < 1 or slot in seen_slots
                    or not isinstance(raw, bytes) or not raw
                    or not image.get("acquired_at") or not image.get("provenance")):
                raise ValueError("Invalid image evidence")
            seen_slots.add(slot)
            sha = digest(raw)
            blobs[sha] = base64.b64encode(raw).decode("ascii")
            image_manifest.append({"slot": slot, "sha256": sha, "size": len(raw),
                                   "acquired_at": image["acquired_at"],
                                   "provenance": image["provenance"]})
        payload = {"format": "job-copy-local-validation/v1", "observation_id": observation_id,
                   "identity": identity, "metadata": metadata,
                   "raw_source": {"sha256": digest(raw_source), "size": len(raw_source)},
                   "images": sorted(image_manifest, key=lambda item: item["slot"])}
        payload_hash = digest(canonical(payload))
        path = self.root / "operations" / f"{observation_id}.json"
        with self._lock():
            if path.exists():
                existing = json.loads(path.read_bytes())
                if existing["payload_hash"] != payload_hash:
                    raise Conflict("Observation identity already has different evidence")
                return observation_id
            operation = {"operation_id": observation_id, "state": "pending", "attempts": 0,
                         "last_error": None, "payload_hash": payload_hash,
                         "payload": payload, "blobs": blobs}
            self._atomic(path, canonical(operation))
        return observation_id

    def _checkpoint(self, phase: str):
        """Fault injection hook for tests; actual store has no external side effect."""

    def _commit(self, operation: dict):
        payload = operation["payload"]
        if digest(canonical(payload)) != operation["payload_hash"]:
            raise Conflict("Corrupted pending payload")
        if (digest(canonical(payload["identity"])) != payload["observation_id"]
                or operation["operation_id"] != payload["observation_id"]):
            raise Conflict("Corrupted operation identity")
        references = [payload["raw_source"], *payload["images"]]
        for reference in references:
            sha = reference["sha256"]
            self._identifier(sha)
            if sha not in operation["blobs"]:
                raise Conflict("Pending operation is missing evidence")
            raw = base64.b64decode(operation["blobs"][sha], validate=True)
            if digest(raw) != sha or len(raw) != reference["size"]:
                raise Conflict("Pending evidence integrity check failed")
        for sha, encoded in operation["blobs"].items():
            self._identifier(sha)
            raw = base64.b64decode(encoded, validate=True)
            if digest(raw) != sha:
                raise Conflict("Corrupted raw evidence")
            self._immutable(self.root / "blobs" / sha, raw)
        self._checkpoint("after_blobs")
        identifier = payload["observation_id"]
        self._identifier(identifier)
        self._immutable(self.root / "observations" / f"{identifier}.json", canonical(payload))
        self._checkpoint("after_manifest")

    def recover_pending(self) -> dict:
        """Only pending snapshots replay; completed/failed records are receipts.

        Local I/O failures remain pending; corruption/conflict stops automatic
        retries. Lock serializes local commits, including concurrent processes.
        """
        counts = {"completed": 0, "pending": 0, "failed": 0}
        with self._lock():
            for path in sorted((self.root / "operations").glob("*.json")):
                operation = json.loads(path.read_bytes())
                if operation["state"] != "pending":
                    continue
                operation["attempts"] += 1
                try:
                    self._commit(operation)
                except (Conflict, ValueError):
                    operation.update(state="failed", last_error="evidence_conflict_or_corruption")
                except OSError:
                    operation.update(state="pending", last_error="local_io_failure")
                else:
                    operation.update(state="completed", last_error=None)
                    # Raw bytes now live in content-addressed blobs. Receipt
                    # retains immutable payload and hash, not duplicate bytes.
                    operation.pop("blobs", None)
                self._atomic(path, canonical(operation))
                counts[operation["state"]] += 1
        return counts

    def read_observation(self, observation_id: str) -> dict:
        self._identifier(observation_id)
        path = self.root / "observations" / f"{observation_id}.json"
        payload = json.loads(path.read_bytes())
        for reference in [payload["raw_source"], *payload["images"]]:
            sha = reference["sha256"]
            self._identifier(sha)
            raw = (self.root / "blobs" / sha).read_bytes()
            if digest(raw) != sha or len(raw) != reference["size"]:
                raise Conflict("Committed evidence integrity check failed")
        return payload


def run_synthetic_validation() -> dict:
    """Create reviewable artifacts inside the existing ignored local directory."""
    from datetime import datetime, timezone
    import uuid
    trial = DEFAULT_ROOT / f"synthetic-{uuid.uuid4().hex}"

    class Interrupted(LocalSnapshotStore):
        def _checkpoint(self, phase):
            if phase == "after_manifest":
                raise OSError("Synthetic acknowledgement failure")

    store = Interrupted(trial)
    identity = {"media": "synthetic", "account_key": "test-account", "media_job_id": "test-job",
                "batch_id": "synthetic-001", "observed_at": "2026-10-04T00:00:00Z"}
    first = store.enqueue(identity, {"source_quality": "synthetic"}, b"synthetic original A")
    interrupted = store.recover_pending()
    restarted = LocalSnapshotStore(trial)
    recovered = restarted.recover_pending()
    repeated = restarted.enqueue(identity, {"source_quality": "synthetic"}, b"synthetic original A")
    second = restarted.enqueue({**identity, "batch_id": "synthetic-002"},
                               {"source_quality": "synthetic"}, b"synthetic original B")
    restarted.recover_pending()
    for identifier in (first, second):
        restarted.read_observation(identifier)
    proof = {"validation_only": True, "executed_at": datetime.now(timezone.utc).isoformat(),
             "interrupted": interrupted, "recovered": recovered,
             "same_observation_retry_id": first == repeated,
             "observation_count": len(list((trial / "observations").glob("*.json"))),
             "pending_only_second_recovery": restarted.recover_pending(),
             "external_requests": 0, "hubspot_writes": 0}
    restarted._atomic(trial / "validation-evidence.json", canonical(proof))
    return {**proof, "local_artifact_directory": str(trial)}


if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--demo", action="store_true", help="Write synthetic local validation artifacts")
    arguments = parser.parse_args()
    if arguments.demo:
        print(json.dumps(run_synthetic_validation(), ensure_ascii=False))
