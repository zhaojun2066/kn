#!/usr/bin/env python3
"""Tests for --json output mode of the profile CLI."""

import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest

PROFILE_CLI = os.path.join(os.path.dirname(__file__), "..", "bin", "profile")

# Template for a minimal config.yaml with a "deepseek" test profile
_MINIMAL_CONFIG = """default: deepseek

profiles:
  deepseek:
    desc: Test DeepSeek profile
    env:
      ANTHROPIC_AUTH_TOKEN: sk-test-token-12345
      ANTHROPIC_BASE_URL: https://api.deepseek.com
      ANTHROPIC_MODEL: deepseek-v4-pro
  claude-config:
    desc: Test Claude profile
    env:
      ANTHROPIC_AUTH_TOKEN: sk-claude-test-token
      ANTHROPIC_BASE_URL: https://api.anthropic.com
      ANTHROPIC_MODEL: claude-sonnet-4-6
"""


def run_cli(*args, stdin_data=None):
    """Run profile CLI and return (returncode, stdout, stderr)."""
    result = subprocess.run(
        [sys.executable, PROFILE_CLI] + list(args),
        input=stdin_data,
        capture_output=True,
        text=True,
    )
    return result.returncode, result.stdout, result.stderr


class TempConfigMixin:
    """Mixin that creates a temporary config directory with a known profile."""

    @classmethod
    def setUpClass(cls):
        cls._tmpdir = tempfile.mkdtemp(prefix="kn-test-json-")
        # Write a minimal config.yaml so "deepseek" profile always exists
        os.makedirs(cls._tmpdir, exist_ok=True)
        cfg = os.path.join(cls._tmpdir, "config.yaml")
        with open(cfg, "w") as f:
            f.write(_MINIMAL_CONFIG)
        os.environ["CLAUDE_PROFILES_HOME"] = cls._tmpdir

    @classmethod
    def tearDownClass(cls):
        shutil.rmtree(cls._tmpdir, ignore_errors=True)
        os.environ.pop("CLAUDE_PROFILES_HOME", None)


class TestListJson(TempConfigMixin, unittest.TestCase):
    def test_valid_json_output(self):
        rc, stdout, stderr = run_cli("--json", "list")
        self.assertEqual(rc, 0)
        data = json.loads(stdout)
        self.assertIn("profiles", data)
        self.assertIsInstance(data["profiles"], list)
        self.assertIn("default", data)

    def test_profile_structure(self):
        rc, stdout, stderr = run_cli("--json", "list")
        data = json.loads(stdout)
        for p in data["profiles"]:
            self.assertIn("name", p)
            self.assertIn("desc", p)
            self.assertIn("env_count", p)
            self.assertIn("is_default", p)
            self.assertIsInstance(p["env_count"], int)
            self.assertIsInstance(p["is_default"], bool)

    def test_default_marker_consistent(self):
        rc, stdout, stderr = run_cli("--json", "list")
        data = json.loads(stdout)
        default_count = sum(1 for p in data["profiles"] if p["is_default"])
        if data["default"]:
            self.assertGreaterEqual(default_count, 1)


class TestShowJson(TempConfigMixin, unittest.TestCase):
    def test_valid_json_output(self):
        rc, stdout, stderr = run_cli("--json", "show", "deepseek")
        self.assertEqual(rc, 0)
        data = json.loads(stdout)
        self.assertEqual(data["name"], "deepseek")
        self.assertIn("env", data)
        self.assertIsInstance(data["env"], dict)

    def test_missing_profile(self):
        rc, stdout, stderr = run_cli("--json", "show", "nonexistent")
        self.assertNotEqual(rc, 0)
        err = json.loads(stderr)
        self.assertFalse(err["ok"])
        self.assertIn("error", err)

    def test_unmasked_values(self):
        """JSON mode should output raw values (no masking)."""
        rc, stdout, stderr = run_cli("--json", "show", "deepseek")
        data = json.loads(stdout)
        token = data["env"].get("ANTHROPIC_AUTH_TOKEN", "")
        self.assertNotIn("****", token)


class TestEnvJson(TempConfigMixin, unittest.TestCase):
    def test_valid_json_output(self):
        rc, stdout, stderr = run_cli("--json", "env", "deepseek")
        self.assertEqual(rc, 0)
        data = json.loads(stdout)
        self.assertEqual(data["name"], "deepseek")
        self.assertIn("env", data)
        self.assertIn("ANTHROPIC_AUTH_TOKEN", data["env"])

    def test_missing_profile(self):
        rc, stdout, stderr = run_cli("--json", "env", "nonexistent")
        self.assertNotEqual(rc, 0)
        err = json.loads(stderr)
        self.assertFalse(err["ok"])


class TestSetStdin(TempConfigMixin, unittest.TestCase):
    def test_set_via_stdin(self):
        # Set a known value
        rc, stdout, stderr = run_cli(
            "--json", "--stdin", "set", "deepseek",
            stdin_data="ANTHROPIC_MODEL=stdin-test-value",
        )
        result = json.loads(stdout)
        self.assertEqual(result.get("ok"), True)
        self.assertEqual(result.get("action"), "set")

        # Verify it was set
        rc, stdout, stderr = run_cli("--json", "show", "deepseek")
        data = json.loads(stdout)
        self.assertEqual(data["env"].get("ANTHROPIC_MODEL"), "stdin-test-value")

        # Revert
        run_cli("--json", "--stdin", "set", "deepseek",
                stdin_data="ANTHROPIC_MODEL=deepseek-v4-pro")

    def test_stdin_with_special_chars(self):
        tricky_value = "https://api.example.com/v1?key=val&token=abc123#section"
        rc, stdout, stderr = run_cli(
            "--json", "--stdin", "set", "deepseek",
            stdin_data=f"TEST_URL={tricky_value}",
        )
        result = json.loads(stdout)
        self.assertEqual(result.get("ok"), True)

        # Verify
        rc, stdout, stderr = run_cli("--json", "show", "deepseek")
        data = json.loads(stdout)
        self.assertEqual(data["env"].get("TEST_URL"), tricky_value)

        # Clean up
        run_cli("--json", "unset", "deepseek", "TEST_URL")


class TestMutationJson(unittest.TestCase):
    def setUp(self):
        # Ensure test profile doesn't exist first
        subprocess.run(
            [sys.executable, PROFILE_CLI, "remove", "jstest"],
            capture_output=True,
        )

    def test_add_and_remove_json(self):
        # Add
        rc, stdout, stderr = run_cli("--json", "add", "jstest", "JSON test")
        self.assertEqual(rc, 0)
        result = json.loads(stdout)
        self.assertTrue(result["ok"])
        self.assertEqual(result["profile"], "jstest")

        # Remove
        rc, stdout, stderr = run_cli("--json", "remove", "jstest")
        self.assertEqual(rc, 0)
        result = json.loads(stdout)
        self.assertTrue(result["ok"])
        self.assertEqual(result["profile"], "jstest")


class TestBackwardCompat(TempConfigMixin, unittest.TestCase):
    """Ensure human-readable output is unchanged."""

    def test_list_human_readable(self):
        rc, stdout, stderr = run_cli("list")
        self.assertEqual(rc, 0)
        self.assertIn("deepseek", stdout)
        self.assertNotIn("{", stdout)  # Not JSON

    def test_show_human_readable(self):
        rc, stdout, stderr = run_cli("show", "deepseek")
        self.assertEqual(rc, 0)
        self.assertIn("[deepseek]", stdout)
        self.assertIn("****", stdout)  # Secrets masked

    def test_env_human_readable(self):
        rc, stdout, stderr = run_cli("env", "deepseek")
        self.assertEqual(rc, 0)
        self.assertIn("export ", stdout)
        self.assertNotIn("{", stdout)


if __name__ == "__main__":
    unittest.main()
