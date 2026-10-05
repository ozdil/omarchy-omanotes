#!/usr/bin/env python3
"""
Automated Ownership & Manifest Verification Tests for OmaNotes build.sh and uninstall.sh
Tests symlink refusal, foreign file preservation, manifest creation, and safe uninstallation.
"""

import os
import subprocess
import tempfile
import unittest


class TestInstallerManifest(unittest.TestCase):
    def setUp(self):
        self.test_dir = tempfile.TemporaryDirectory()
        self.home = self.test_dir.name
        self.env = os.environ.copy()
        self.env["HOME"] = self.home
        self.bin_dir = os.path.join(self.home, ".local/bin")
        self.state_dir = os.path.join(self.home, ".local/state/omarchy/omanotes")
        os.makedirs(self.bin_dir, exist_ok=True)
        os.makedirs(self.state_dir, exist_ok=True)
        self.repo_dir = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))

    def tearDown(self):
        self.test_dir.cleanup()

    def test_foreign_symlink_refusal(self):
        # Create a symlink in ~/.local/bin/omanotes pointing to a dummy target
        dummy = os.path.join(self.home, "important_doc.txt")
        with open(dummy, "w") as f:
            f.write("DO NOT OVERWRITE")
        symlink_target = os.path.join(self.bin_dir, "omanotes")
        os.symlink(dummy, symlink_target)

        # Running build.sh MUST fail and refuse to overwrite symlink
        proc = subprocess.run(
            ["/bin/bash", "-p", os.path.join(self.repo_dir, "build.sh")],
            env=self.env,
            cwd=self.repo_dir,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        self.assertNotEqual(proc.returncode, 0, "build.sh must fail when target is a symlink")
        self.assertIn("Security Error", proc.stderr)
        self.assertTrue(os.path.islink(symlink_target))
        with open(dummy, "r") as f:
            self.assertEqual(f.read(), "DO NOT OVERWRITE")

    def test_foreign_file_refusal(self):
        # Create an existing manifest that does NOT include omanotes-dashboard
        manifest = os.path.join(self.state_dir, "install_manifest.json")
        with open(manifest, "w") as f:
            f.write('{"files": {"/other/path": "1234"}}\n')

        foreign_bin = os.path.join(self.bin_dir, "omanotes-dashboard")
        with open(foreign_bin, "w") as f:
            f.write("FOREIGN_BINARY_CONTENT")

        # Running build.sh MUST fail and refuse to overwrite foreign user-managed binary
        proc = subprocess.run(
            ["/bin/bash", "-p", os.path.join(self.repo_dir, "build.sh")],
            env=self.env,
            cwd=self.repo_dir,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        self.assertNotEqual(proc.returncode, 0, "build.sh must fail when foreign file exists")
        self.assertIn("Security Conflict", proc.stderr)
        with open(foreign_bin, "r") as f:
            self.assertEqual(f.read(), "FOREIGN_BINARY_CONTENT")

    def test_untracked_target_refusal_when_no_manifest(self):
        # Create an existing target file in ~/.local/bin before OmaNotes creates any manifest
        foreign_bin = os.path.join(self.bin_dir, "omanotes-status")
        with open(foreign_bin, "w") as f:
            f.write("EXISTING_UNTRACKED_FILE")

        # Running build.sh MUST fail and refuse to overwrite untracked pre-existing file
        proc = subprocess.run(
            ["/bin/bash", "-p", os.path.join(self.repo_dir, "build.sh")],
            env=self.env,
            cwd=self.repo_dir,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        self.assertNotEqual(proc.returncode, 0, "build.sh must fail when untracked file exists without manifest")
        self.assertIn("Security Conflict", proc.stderr)
        with open(foreign_bin, "r") as f:
            self.assertEqual(f.read(), "EXISTING_UNTRACKED_FILE")

    def test_clean_install_and_uninstall_lifecycle(self):
        # 1. Clean build
        proc = subprocess.run(
            ["/bin/bash", "-p", os.path.join(self.repo_dir, "build.sh")],
            env=self.env,
            cwd=self.repo_dir,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        self.assertEqual(proc.returncode, 0, f"build.sh failed: {proc.stderr}")

        manifest = os.path.join(self.state_dir, "install_manifest.json")
        self.assertTrue(os.path.exists(manifest))
        self.assertTrue(os.path.exists(os.path.join(self.bin_dir, "omanotes-engine")))
        self.assertTrue(os.path.exists(os.path.join(self.bin_dir, "omanotes")))

        # 2. Add an unmanaged user file
        unrelated_file = os.path.join(self.bin_dir, "my_custom_script")
        with open(unrelated_file, "w") as f:
            f.write("echo 'keep me'")

        # 3. Clean uninstall
        proc_un = subprocess.run(
            ["/bin/bash", "-p", os.path.join(self.repo_dir, "uninstall.sh")],
            env=self.env,
            cwd=self.repo_dir,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        self.assertEqual(proc_un.returncode, 0, f"uninstall.sh failed: {proc_un.stderr}")

        # OmaNotes binaries removed
        self.assertFalse(os.path.exists(os.path.join(self.bin_dir, "omanotes-engine")))
        self.assertFalse(os.path.exists(os.path.join(self.bin_dir, "omanotes")))
        self.assertFalse(os.path.exists(manifest))

        # Unrelated user file strictly preserved
        self.assertTrue(os.path.exists(unrelated_file))
        with open(unrelated_file, "r") as f:
            self.assertEqual(f.read(), "echo 'keep me'")

    def test_tampered_file_hash_mismatch(self):
        # Create manifest with an expected hash for omanotes-dashboard
        manifest = os.path.join(self.state_dir, "install_manifest.json")
        target_bin = os.path.join(self.bin_dir, "omanotes-dashboard")
        with open(manifest, "w") as f:
            f.write(f'{{"files": {{"{target_bin}": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"}}}}\n')

        with open(target_bin, "w") as f:
            f.write("MODIFIED_OR_TAMPERED_CONTENT")

        # Running build.sh MUST fail due to hash mismatch
        proc = subprocess.run(
            ["/bin/bash", "-p", os.path.join(self.repo_dir, "build.sh")],
            env=self.env,
            cwd=self.repo_dir,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        self.assertEqual(proc.returncode, 1, "build.sh must exit 1 when target file hash does not match manifest")
        self.assertIn("Security Conflict", proc.stderr)
        self.assertIn("hash does not match", proc.stderr)
        with open(target_bin, "r") as f:
            self.assertEqual(f.read(), "MODIFIED_OR_TAMPERED_CONTENT")


if __name__ == "__main__":
    unittest.main()
