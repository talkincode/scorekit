#!/usr/bin/env python3
"""Tests for scripts/workspace.py. Run: python3 scripts/workspace_test.py"""

from __future__ import annotations

import importlib.util
import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPTS_DIR = Path(__file__).resolve().parent
REPO_ROOT = SCRIPTS_DIR.parent
SCRIPT = SCRIPTS_DIR / "workspace.py"

spec = importlib.util.spec_from_file_location("workspace", SCRIPT)
workspace = importlib.util.module_from_spec(spec)
spec.loader.exec_module(workspace)

GIT_SIBLINGS = ("scorebench", "scorebench-samples", "scoredata-forge")


def run(args, env=None):
    return subprocess.run(
        [sys.executable, str(SCRIPT)] + args,
        capture_output=True, text=True, env=env,
    )


class WorkspaceTest(unittest.TestCase):
    def setUp(self):
        # resolve(): macOS tempdirs live behind the /var -> /private/var symlink
        self.tmp = Path(tempfile.mkdtemp(prefix="scorekit-ws-test-")).resolve()
        self.addCleanup(shutil.rmtree, self.tmp, ignore_errors=True)
        self.root = self.tmp / "root"
        self.repo = self.root / "scorekit"
        self.repo.mkdir(parents=True)
        for name in GIT_SIBLINGS:
            (self.root / name).mkdir()
        self.data = self.tmp / "data" / "ScoreData"
        self.data.mkdir(parents=True)
        # The committed manifest is itself the test fixture: tests break if it drifts.
        shutil.copy(REPO_ROOT / workspace.MANIFEST_NAME, self.repo / workspace.MANIFEST_NAME)
        self.base_env = {"PATH": os.environ.get("PATH", "")}

    def env_with_data(self):
        env = dict(self.base_env)
        env["SCOREKIT_SOUND_LIBRARY_DIR"] = str(self.data)
        return env

    def write_local(self, paths):
        (self.repo / workspace.LOCAL_NAME).write_text(
            json.dumps({"paths": paths}), encoding="utf-8"
        )

    def doctor(self, env=None, extra=()):
        return run(["--repo-root", str(self.repo), "--json", "doctor", *extra],
                   env=env or self.base_env)

    def gen(self, env=None, extra=()):
        return run(["--repo-root", str(self.repo), "gen", *extra],
                   env=env or self.base_env)

    # --- manifest -----------------------------------------------------------

    def test_committed_manifest_is_valid(self):
        manifest = workspace.load_manifest(REPO_ROOT)
        self.assertIn("scorekit", manifest["projects"])
        self.assertIn("ScoreData", manifest["projects"])

    def test_invalid_manifest_exits_2(self):
        (self.repo / workspace.MANIFEST_NAME).write_text('{"version": 1}', encoding="utf-8")
        proc = self.doctor()
        self.assertEqual(proc.returncode, 2)
        self.assertIn("projects", proc.stderr)

    # --- doctor resolution --------------------------------------------------

    def test_doctor_ok_with_env_resolution(self):
        proc = self.doctor(env=self.env_with_data())
        self.assertEqual(proc.returncode, 0, proc.stderr)
        report = json.loads(proc.stdout)
        self.assertTrue(report["ok"])
        by_name = {p["name"]: p for p in report["projects"]}
        self.assertEqual(by_name["scorekit"]["source"], "self")
        self.assertEqual(by_name["scorebench"]["source"], "sibling")
        self.assertEqual(by_name["ScoreData"]["source"], "env")
        self.assertEqual(by_name["ScoreData"]["path"], str(self.data))

    def test_doctor_sibling_data_dir(self):
        (self.root / "ScoreData").mkdir()
        proc = self.doctor()
        report = json.loads(proc.stdout)
        by_name = {p["name"]: p for p in report["projects"]}
        self.assertEqual(by_name["ScoreData"]["source"], "sibling")
        self.assertTrue(report["ok"])

    def test_doctor_override_beats_env(self):
        self.write_local({"ScoreData": str(self.data)})
        env = dict(self.base_env)
        env["SCOREKIT_SOUND_LIBRARY_DIR"] = str(self.tmp / "nonexistent")
        proc = self.doctor(env=env)
        self.assertEqual(proc.returncode, 0, proc.stdout)
        by_name = {p["name"]: p for p in json.loads(proc.stdout)["projects"]}
        self.assertEqual(by_name["ScoreData"]["source"], "override")

    def test_doctor_missing_repo_hints_clone(self):
        shutil.rmtree(self.root / "scorebench")
        proc = self.doctor(env=self.env_with_data())
        self.assertEqual(proc.returncode, 1)
        report = json.loads(proc.stdout)
        self.assertFalse(report["ok"])
        hint = {p["name"]: p for p in report["projects"]}["scorebench"]["hint"]
        self.assertIn("git clone https://github.com/talkincode/scorebench ", hint)

    def test_doctor_unknown_override_key_exits_2(self):
        self.write_local({"typo-project": "/abs/path"})
        proc = self.doctor(env=self.env_with_data())
        self.assertEqual(proc.returncode, 2)
        self.assertIn("typo-project", proc.stderr)

    def test_doctor_relative_override_exits_2(self):
        self.write_local({"ScoreData": "relative/path"})
        proc = self.doctor()
        self.assertEqual(proc.returncode, 2)

    # --- gen ----------------------------------------------------------------

    def ws_file(self):
        return self.root / workspace.CODE_WORKSPACE_NAME

    def env_file(self):
        return self.root / workspace.ENV_FRAGMENT_NAME

    def test_gen_fail_closed_writes_nothing(self):
        shutil.rmtree(self.root / "scorebench-samples")
        proc = self.gen(env=self.env_with_data())
        self.assertEqual(proc.returncode, 1)
        self.assertFalse(self.ws_file().exists(), "partial artifact left behind")
        self.assertFalse(self.env_file().exists(), "partial artifact left behind")

    def test_gen_writes_deterministic_artifacts(self):
        proc = self.gen(env=self.env_with_data())
        self.assertEqual(proc.returncode, 0, proc.stderr)
        first = self.ws_file().read_bytes()
        doc = json.loads(first)
        folders = {f["name"]: f["path"] for f in doc["folders"]}
        self.assertEqual(len(folders), 5)
        self.assertEqual(folders["scorekit"], "scorekit")
        self.assertEqual(folders["scorebench-samples"], "scorebench-samples")
        self.assertEqual(folders["ScoreData"], os.path.join("..", "data", "ScoreData"))
        env_text = self.env_file().read_text(encoding="utf-8")
        self.assertIn(f'export SCOREKIT_SOUND_LIBRARY_DIR="{self.data}"', env_text)
        # Re-run: byte-identical and reported unchanged.
        proc2 = self.gen(env=self.env_with_data())
        self.assertEqual(proc2.returncode, 0)
        self.assertIn("unchanged", proc2.stdout)
        self.assertEqual(first, self.ws_file().read_bytes())

    def test_gen_allow_missing_skips(self):
        shutil.rmtree(self.root / "scoredata-forge")
        proc = self.gen(env=self.env_with_data(), extra=("--allow-missing",))
        self.assertEqual(proc.returncode, 0, proc.stderr)
        doc = json.loads(self.ws_file().read_text(encoding="utf-8"))
        names = [f["name"] for f in doc["folders"]]
        self.assertNotIn("scoredata-forge", names)
        self.assertEqual(len(names), 4)

    def test_gen_preserves_existing_settings(self):
        self.ws_file().write_text(
            json.dumps({"folders": [], "settings": {"editor.formatOnSave": True}}),
            encoding="utf-8",
        )
        proc = self.gen(env=self.env_with_data())
        self.assertEqual(proc.returncode, 0, proc.stderr)
        doc = json.loads(self.ws_file().read_text(encoding="utf-8"))
        self.assertEqual(doc["settings"], {"editor.formatOnSave": True})
        self.assertEqual(len(doc["folders"]), 5)


if __name__ == "__main__":
    unittest.main()
