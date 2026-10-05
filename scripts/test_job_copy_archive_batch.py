"""Offline orchestration checks. All transports and write pilots are mocked."""
import contextlib
import copy
import hashlib
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from PIL import Image

import job_copy_archive_batch as batch
from job_copy_drive_pilot import PilotError


class ArchiveBatchTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.samples = {"samples": []}
        self.sizes = []
        for index, color in enumerate(("red", "blue")):
            path = self.root / (str(index) + ".png")
            Image.new("RGB", (2, 3), color=color).save(path, format="PNG")
            raw = path.read_bytes()
            self.sizes.append(len(raw))
            self.samples["samples"].append({"listing_id": str(30 + index), "company_ids": ["10"],
                "observed_at": "2026-10-05T01:02:03Z", "source_slot": 1,
                "source_path": str(path), "sha256": hashlib.sha256(raw).hexdigest(), "bytes": len(raw)})
        # Shared original in a different listing: image CREATE must dedupe by hash.
        self.samples["samples"].append({**self.samples["samples"][0], "source_slot": 2, "listing_id": "31"})
        self.config = {"schemaVersion": 1, "folderId": "synthetic_folder", "identityFolderId": "synthetic_identity",
                       "imageReceiptDir": str(self.root / "images"), "manifestReceiptDir": str(self.root / "manifests")}
        self.output = self.root / "output"
        self.images, self.manifests, self.events = {}, {}, []
        self.fail_image = None
        self.fail_manifest = None
        owner = self

        class Images:
            def __init__(self, *args, **kwargs):
                self.create_attempts = 0

            def ensure_image(self, path, upload=False):
                running = owner.read("summary.json")
                owner.assertEqual(running["state"], "running")
                owner.assertFalse(running["ok"])
                owner.assertTrue(running["runId"])
                owner.assertTrue(running["startedAt"])
                raw = Path(path).read_bytes()
                digest = hashlib.sha256(raw).hexdigest()
                owner.events.append(("image", digest, upload))
                if digest == owner.fail_image:
                    raise PilotError("mock_image_failure")
                if digest not in owner.images:
                    self.create_attempts += 1
                    owner.images[digest] = {"file_id": "synthetic_image_" + str(len(owner.images)),
                        "sha256": digest, "original_bytes": len(raw), "readback_hash_matches": True}
                return copy.deepcopy(owner.images[digest])

        class Manifests:
            def __init__(self, *args, **kwargs):
                self.create_attempts = 0

            def ensure_manifest(self, manifest, upload=False):
                owner.assertEqual(owner.read("summary.json")["state"], "running")
                listing = manifest["listingId"]
                owner.events.append(("manifest", listing, upload))
                if listing == owner.fail_manifest:
                    raise PilotError("mock_manifest_failure")
                if listing not in owner.manifests:
                    self.create_attempts += 1
                    owner.manifests[listing] = {"listing_id": listing, "readback_hash_matches": True}
                return copy.deepcopy(owner.manifests[listing])

        self.image_class, self.manifest_class = Images, Manifests

    def run_archive(self):
        with patch.object(batch, "GwsCli") as cli, patch.object(batch, "ServiceReadbackCli") as transport, \
             patch.object(batch, "DrivePilot", self.image_class), patch.object(batch, "DriveManifestPilot", self.manifest_class), \
             contextlib.redirect_stdout(io.StringIO()):
            result = batch.archive(self.samples, self.config, "synthetic_key", self.output, upload=True)
            cli.return_value.call.assert_not_called()
            transport.return_value.call.assert_not_called()
            return result

    def read(self, name):
        return json.loads((self.output / name).read_text(encoding="utf-8"))

    def test_dedupe_complete_manifests_summary_and_no_hubspot(self):
        summary = self.run_archive()
        self.assertEqual(summary["unique_originals"], 2)
        self.assertEqual(summary["image_references"], 3)
        self.assertEqual(summary["original_bytes"], sum(self.sizes))
        self.assertEqual(summary["manifests"], 2)
        self.assertEqual((summary["image_create_attempts"], summary["manifest_create_attempts"]), (2, 2))
        self.assertEqual(summary["hubspot_writes"], 0)
        self.assertFalse(summary["permissions_changed"])
        self.assertEqual([event[0] for event in self.events], ["image", "image", "manifest", "manifest"])
        self.assertEqual(self.read("summary.json"), summary)

    def test_image_failure_checkpoint_then_receipt_resume_no_duplicate_create(self):
        self.fail_image = self.samples["samples"][1]["sha256"]
        with self.assertRaisesRegex(PilotError, "mock_image_failure"):
            self.run_archive()
        self.assertEqual(len(self.read("upload-results.json")), 1)
        failure = self.read("last-failure.json")
        self.assertEqual({key: failure[key] for key in ("phase", "error", "verified_originals", "verified_manifests")},
                         {"phase": "images", "error": "mock_image_failure", "verified_originals": 1, "verified_manifests": 0})
        self.assertEqual(self.read("summary.json"), failure)
        self.assertFalse(failure["ok"])
        self.assertEqual(failure["state"], "failed")
        self.assertFalse(self.manifests)
        self.fail_image = None
        summary = self.run_archive()
        self.assertEqual(summary["image_create_attempts"], 1)
        self.assertEqual(len(self.images), 2)
        self.assertFalse((self.output / "last-failure.json").exists())

    def test_previous_success_then_failure_replaces_status_then_resume_clears_failure(self):
        success = self.run_archive()
        self.fail_image = self.samples["samples"][0]["sha256"]
        with self.assertRaises(PilotError):
            self.run_archive()
        failure = self.read("summary.json")
        self.assertFalse(failure["ok"])
        self.assertEqual(failure["state"], "failed")
        self.assertNotEqual(failure["runId"], success["runId"])
        self.assertEqual(failure, self.read("last-failure.json"))
        # Earlier readback checkpoints remain durable recovery evidence.
        self.assertEqual(len(self.read("upload-results.json")), 2)
        self.assertEqual(len(self.read("manifest-results.json")["results"]), 2)
        self.fail_image = None
        resumed = self.run_archive()
        self.assertTrue(resumed["ok"])
        self.assertEqual(resumed["state"], "succeeded")
        self.assertNotEqual(resumed["runId"], failure["runId"])
        self.assertFalse((self.output / "last-failure.json").exists())

    def test_manifest_failure_checkpoint_resume_and_completed_rerun(self):
        self.fail_manifest = "31"
        with self.assertRaisesRegex(PilotError, "mock_manifest_failure"):
            self.run_archive()
        self.assertEqual(len(self.read("manifest-results.json")["results"]), 1)
        failure = self.read("last-failure.json")
        self.assertEqual((failure["phase"], failure["verified_originals"], failure["verified_manifests"]), ("manifests", 2, 1))
        self.fail_manifest = None
        summary = self.run_archive()
        self.assertEqual((summary["image_create_attempts"], summary["manifest_create_attempts"]), (0, 1))
        again = self.run_archive()
        self.assertEqual((again["image_create_attempts"], again["manifest_create_attempts"]), (0, 0))
        self.assertEqual((len(self.images), len(self.manifests)), (2, 2))

    def test_bad_input_rejected_before_any_external_constructor_or_checkpoint(self):
        for mutate in (
            lambda: self.samples["samples"][0].update(sha256="bad"),
            lambda: self.samples["samples"][0].update(bytes=123456),
            lambda: self.samples["samples"][0].update(source_path=str(self.root / "missing.png")),
        ):
            original = copy.deepcopy(self.samples)
            mutate()
            with patch.object(batch, "GwsCli") as cli, patch.object(batch, "ServiceReadbackCli") as transport:
                with self.assertRaises(PilotError):
                    batch.archive(self.samples, self.config, "synthetic_key", self.output, upload=True)
                cli.assert_not_called()
                transport.assert_not_called()
            self.assertFalse(self.output.exists())
            self.samples = original

    def test_actual_private_preflight_arithmetic_if_available(self):
        path = Path("data/job-copy-local/applicant-review/expansion/samples.json")
        if not path.is_file():
            self.skipTest("Private expansion evidence is intentionally untracked")
        samples = json.loads(path.read_text(encoding="utf-8"))
        unique, manifests = batch.preflight(samples, "synthetic_folder", "synthetic_identity")
        self.assertEqual((len(samples["samples"]), len(unique), manifests), (45, 34, 28))
        self.assertEqual(sum(row["bytes"] for row in unique), 7130985)
        self.assertEqual(sum(row["bytes"] for row in samples["samples"]), 8986585)


if __name__ == "__main__":
    unittest.main()
