#!/usr/bin/env python3
"""Tests for profile init — importing from Claude Code + Codex configs."""

import io
import json
import os
import sys
import tempfile
import unittest
from contextlib import contextmanager
from unittest.mock import patch, mock_open

# Load modules
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "lib"))
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "bin"))
import config as cfg

# Import the profile module (CLI script, no .py extension — load via SourceFileLoader)
import importlib.machinery
_profile_path = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "bin", "profile"))
loader = importlib.machinery.SourceFileLoader("profile", _profile_path)
spec = importlib.util.spec_from_loader("profile", loader)
profile = importlib.util.module_from_spec(spec)
loader.exec_module(profile)


class TestImportClaudeSettings(unittest.TestCase):
    """Tests for _import_claude_settings()."""

    def setUp(self):
        self.tmp = tempfile.NamedTemporaryFile(mode="w", suffix=".yaml", delete=False)
        self.lock = tempfile.NamedTemporaryFile(mode="w", suffix=".lock", delete=False)
        cfg.CONFIG_FILE = self.tmp.name
        cfg.LOCK_FILE = self.lock.name
        # Start with empty config
        cfg.write_config({"profiles": {}})

    def tearDown(self):
        self.tmp.close()
        self.lock.close()
        os.unlink(self.tmp.name)
        os.unlink(self.lock.name)

    @contextmanager
    def _mock_settings_json(self, content=None):
        """Patch os.path.expanduser to return temp paths for settings.json."""
        tmpdir = tempfile.mkdtemp()
        try:
            if content is not None:
                settings_path = os.path.join(tmpdir, ".claude", "settings.json")
                os.makedirs(os.path.dirname(settings_path), exist_ok=True)
                with open(settings_path, "w") as f:
                    f.write(content)

            def fake_expanduser(p):
                if p == "~/.claude/settings.json":
                    if content is not None:
                        return settings_path
                return os.path.join(tmpdir, p.replace("~/", ""))

            with patch("os.path.expanduser", side_effect=fake_expanduser):
                yield
        finally:
            import shutil
            shutil.rmtree(tmpdir, ignore_errors=True)

    def test_no_settings_file(self):
        """When settings.json doesn't exist, return False."""
        config = cfg.read_config()
        with self._mock_settings_json(content=None):
            result = profile._import_claude_settings(config)
        self.assertFalse(result)

    def test_empty_env(self):
        """When settings.json has empty env, return False."""
        config = cfg.read_config()
        with self._mock_settings_json(content='{"env": {}}'):
            result = profile._import_claude_settings(config)
        self.assertFalse(result)

    def test_deepseek_detection(self):
        """Profile named 'deepseek' when base_url contains 'deepseek'."""
        config = cfg.read_config()
        s = json.dumps({"env": {
            "ANTHROPIC_AUTH_TOKEN": "sk-1234abcd",
            "ANTHROPIC_BASE_URL": "https://api.deepseek.com/anthropic",
            "ANTHROPIC_MODEL": "deepseek-v4-pro",
        }})
        with self._mock_settings_json(content=s):
            result = profile._import_claude_settings(config)
        self.assertTrue(result)
        config = cfg.read_config()
        self.assertIn("deepseek", config["profiles"])
        self.assertEqual(
            config["profiles"]["deepseek"]["env"]["ANTHROPIC_BASE_URL"],
            "https://api.deepseek.com/anthropic",
        )
        self.assertEqual(config["default"], "deepseek")

    def test_generic_imported_name(self):
        """Arbitrary base_url → profile named 'imported'."""
        config = cfg.read_config()
        s = json.dumps({"env": {
            "ANTHROPIC_AUTH_TOKEN": "sk-abc",
            "ANTHROPIC_BASE_URL": "https://api.custom.com/anthropic",
        }})
        with self._mock_settings_json(content=s):
            result = profile._import_claude_settings(config)
        self.assertTrue(result)
        config = cfg.read_config()
        self.assertIn("imported", config["profiles"])

    def test_preserves_existing_default(self):
        """Don't overwrite existing default profile."""
        config = cfg.read_config()
        config["default"] = "existing"
        config["profiles"]["existing"] = {"desc": "keep me", "env": {"X": "1"}}

        s = json.dumps({"env": {
            "ANTHROPIC_AUTH_TOKEN": "sk-new",
            "ANTHROPIC_BASE_URL": "https://api.deepseek.com/anthropic",
        }})
        with self._mock_settings_json(content=s):
            result = profile._import_claude_settings(config)
        self.assertTrue(result)
        config = cfg.read_config()
        # Should NOT overwrite existing default
        self.assertEqual(config["default"], "existing")


class TestImportCodexConfig(unittest.TestCase):
    """Tests for _import_codex_config()."""

    def setUp(self):
        self.tmp = tempfile.NamedTemporaryFile(mode="w", suffix=".yaml", delete=False)
        self.lock = tempfile.NamedTemporaryFile(mode="w", suffix=".lock", delete=False)
        cfg.CONFIG_FILE = self.tmp.name
        cfg.LOCK_FILE = self.lock.name
        cfg.write_config({"profiles": {}})

    def tearDown(self):
        self.tmp.close()
        self.lock.close()
        os.unlink(self.tmp.name)
        os.unlink(self.lock.name)

    @contextmanager
    def _mock_codex_files(self, toml_content=None, auth_json_content=None):
        """Create temp Codex config files and patch expanduser."""
        tmpdir = tempfile.mkdtemp()
        try:
            codex_dir = os.path.join(tmpdir, ".codex")
            os.makedirs(codex_dir, exist_ok=True)
            toml_path = os.path.join(codex_dir, "config.toml")
            auth_path = os.path.join(codex_dir, "auth.json")

            if toml_content is not None:
                with open(toml_path, "w") as f:
                    f.write(toml_content)
            if auth_json_content is not None:
                with open(auth_path, "w") as f:
                    f.write(auth_json_content)

            def fake_expanduser(p):
                mapping = {
                    "~/.codex/config.toml": toml_path if toml_content is not None else os.path.join(tmpdir, "nonexistent"),
                    "~/.codex/auth.json": auth_path if auth_json_content is not None else os.path.join(tmpdir, "nonexistent"),
                }
                return mapping.get(p, os.path.join(tmpdir, p.replace("~/", "")))

            with patch("os.path.expanduser", side_effect=fake_expanduser):
                yield
        finally:
            import shutil
            shutil.rmtree(tmpdir, ignore_errors=True)

    def test_no_codex_files(self):
        """When neither config.toml nor auth.json exist, return False."""
        config = cfg.read_config()
        with self._mock_codex_files(toml_content=None, auth_json_content=None):
            result = profile._import_codex_config(config)
        self.assertFalse(result)

    def test_auth_json_only(self):
        """Only auth.json exists → imports OPENAI_API_KEY."""
        config = cfg.read_config()
        auth = json.dumps({"OPENAI_API_KEY": "sk-test-key-1234"})
        with self._mock_codex_files(toml_content=None, auth_json_content=auth):
            result = profile._import_codex_config(config)
        self.assertTrue(result)
        config = cfg.read_config()
        self.assertIn("codex-imported", config["profiles"])
        self.assertEqual(
            config["profiles"]["codex-imported"]["env"]["OPENAI_API_KEY"],
            "sk-test-key-1234",
        )

    def test_full_codex_import(self):
        """Both auth.json and config.toml → full profile with all vars."""
        config = cfg.read_config()
        auth = json.dumps({"OPENAI_API_KEY": "sk-codex-key"})
        toml = """model = 'gpt-5.5'
model_provider = 'custom'

[model_providers.custom]
base_url = 'http://wucur.com:6543/v1'
name = 'shiba-cc'
wire_api = 'responses'

[shell_environment_policy]
inherit = 'core'

[shell_environment_policy.set]
ANTHROPIC_AUTH_TOKEN = 'sk-anthro-token'
ANTHROPIC_BASE_URL = 'http://wucur.com:6543'
"""
        with self._mock_codex_files(toml_content=toml, auth_json_content=auth):
            result = profile._import_codex_config(config)
        self.assertTrue(result)
        config = cfg.read_config()
        self.assertIn("shiba-cc", config["profiles"])
        env = config["profiles"]["shiba-cc"]["env"]
        self.assertEqual(env["OPENAI_API_KEY"], "sk-codex-key")
        self.assertEqual(env["OPENAI_MODEL"], "gpt-5.5")
        self.assertEqual(env["OPENAI_BASE_URL"], "http://wucur.com:6543/v1")
        self.assertEqual(env["ANTHROPIC_AUTH_TOKEN"], "sk-anthro-token")
        self.assertEqual(env["ANTHROPIC_BASE_URL"], "http://wucur.com:6543")

    def test_config_toml_no_auth_json(self):
        """config.toml exists but no auth.json → imports without OPENAI_API_KEY."""
        config = cfg.read_config()
        toml = """model = 'gpt-5.5'

[model_providers.custom]
base_url = 'http://wucur.com:6543/v1'
name = 'my-provider'

[shell_environment_policy.set]
CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC = '1'
"""
        with self._mock_codex_files(toml_content=toml, auth_json_content=None):
            result = profile._import_codex_config(config)
        self.assertTrue(result)
        config = cfg.read_config()
        env = config["profiles"]["my-provider"]["env"]
        self.assertNotIn("OPENAI_API_KEY", env)
        self.assertEqual(env["OPENAI_MODEL"], "gpt-5.5")
        self.assertEqual(env["CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC"], "1")

    def test_name_collision(self):
        """Existing profile with same name → auto-suffix."""
        config = cfg.read_config()
        config["profiles"]["shiba-cc"] = {"desc": "already exists", "env": {}}

        auth = json.dumps({"OPENAI_API_KEY": "sk-new"})
        toml = """[model_providers.custom]
name = 'shiba-cc'
base_url = 'http://example.com/v1'
"""
        with self._mock_codex_files(toml_content=toml, auth_json_content=auth):
            result = profile._import_codex_config(config)
        self.assertTrue(result)
        config = cfg.read_config()
        # Original profile preserved, new one gets -2 suffix
        self.assertIn("shiba-cc", config["profiles"])
        self.assertIn("shiba-cc-2", config["profiles"])
        self.assertEqual(config["profiles"]["shiba-cc"]["desc"], "already exists")

    def test_strips_wire_api_suffix(self):
        """Base URLs with /responses /chat /completions suffixes are cleaned."""
        config = cfg.read_config()
        for suffix, expected in [
            ("http://api.example.com:6543/v1/responses", "http://api.example.com:6543/v1"),
            ("http://api.example.com:6543/v1/chat", "http://api.example.com:6543/v1"),
            ("http://api.example.com:6543/v1/completions", "http://api.example.com:6543/v1"),
            ("http://api.example.com:6543/v1", "http://api.example.com:6543/v1"),
        ]:
            # Reset
            cfg.write_config({"profiles": {}})
            config = cfg.read_config()
            toml = f"""[model_providers.custom]
name = 'test'
base_url = '{suffix}'
"""
            with self._mock_codex_files(toml_content=toml, auth_json_content=None):
                profile._import_codex_config(config)
            config = cfg.read_config()
            # Find the profile (name collision may add suffix)
            pname = [n for n in config["profiles"] if n.startswith("test")][0]
            self.assertEqual(
                config["profiles"][pname]["env"]["OPENAI_BASE_URL"], expected,
                f"Failed for suffix {suffix}: got {config['profiles'][pname]['env'].get('OPENAI_BASE_URL')}"
            )

    def test_corrupted_toml(self):
        """Malformed TOML → warning, fallback to auth.json if available."""
        config = cfg.read_config()
        auth = json.dumps({"OPENAI_API_KEY": "sk-fallback"})
        toml = "this is not valid {{{{{{{{{{ toml [[[[["
        with self._mock_codex_files(toml_content=toml, auth_json_content=auth):
            result = profile._import_codex_config(config)
        self.assertTrue(result)  # Still succeeds via auth.json fallback
        config = cfg.read_config()
        env = config["profiles"]["codex-imported"]["env"]
        self.assertEqual(env["OPENAI_API_KEY"], "sk-fallback")


class TestCmdInit(unittest.TestCase):
    """Integration tests for cmd_init()."""

    def setUp(self):
        self.tmp = tempfile.NamedTemporaryFile(mode="w", suffix=".yaml", delete=False)
        self.lock = tempfile.NamedTemporaryFile(mode="w", suffix=".lock", delete=False)
        cfg.CONFIG_FILE = self.tmp.name
        cfg.LOCK_FILE = self.lock.name

    def tearDown(self):
        self.tmp.close()
        self.lock.close()
        os.unlink(self.tmp.name)
        os.unlink(self.lock.name)

    def test_no_configs_creates_empty(self):
        """No source configs → prints message, creates empty profiles dict."""
        cfg.write_config({"profiles": {}})
        config = cfg.read_config()

        captured = io.StringIO()
        with patch("sys.stdout", captured):
            with patch.object(profile, "_import_claude_settings", return_value=False):
                with patch.object(profile, "_import_codex_config", return_value=False):
                    profile.cmd_init(config)

        output = captured.getvalue()
        self.assertIn("No existing config found", output)
        config = cfg.read_config()
        self.assertEqual(config["profiles"], {})

    def test_only_claude_imports(self):
        """Only Claude settings exist → one profile created."""
        cfg.write_config({"profiles": {}})
        config = cfg.read_config()

        def fake_claude(c):
            c.setdefault("profiles", {})["deepseek"] = {
                "desc": "test", "env": {"KEY": "val"},
            }
            c["default"] = "deepseek"
            cfg.write_config(c)
            return True

        captured = io.StringIO()
        with patch("sys.stdout", captured):
            with patch.object(profile, "_import_claude_settings", side_effect=fake_claude):
                with patch.object(profile, "_import_codex_config", return_value=False):
                    profile.cmd_init(config)

        config = cfg.read_config()
        self.assertIn("deepseek", config["profiles"])

    def test_only_codex_imports(self):
        """Only Codex configs exist → one profile created."""
        cfg.write_config({"profiles": {}})
        config = cfg.read_config()

        def fake_codex(c):
            c.setdefault("profiles", {})["codex-imported"] = {
                "desc": "test", "env": {"OPENAI_API_KEY": "sk-test"},
            }
            c["default"] = "codex-imported"
            cfg.write_config(c)
            return True

        captured = io.StringIO()
        with patch("sys.stdout", captured):
            with patch.object(profile, "_import_claude_settings", return_value=False):
                with patch.object(profile, "_import_codex_config", side_effect=fake_codex):
                    profile.cmd_init(config)

        config = cfg.read_config()
        self.assertIn("codex-imported", config["profiles"])

    def test_both_sources_imported(self):
        """Both Claude + Codex → two profiles side by side."""
        cfg.write_config({"profiles": {}})
        config = cfg.read_config()

        def fake_claude(c):
            c.setdefault("profiles", {})["deepseek"] = {"desc": "c", "env": {"K": "v"}}
            c["default"] = "deepseek"
            cfg.write_config(c)
            return True

        def fake_codex(c):
            c.setdefault("profiles", {})["codex-imported"] = {"desc": "x", "env": {"K2": "v2"}}
            cfg.write_config(c)
            return True

        captured = io.StringIO()
        with patch("sys.stdout", captured):
            with patch.object(profile, "_import_claude_settings", side_effect=fake_claude):
                with patch.object(profile, "_import_codex_config", side_effect=fake_codex):
                    profile.cmd_init(config)

        config = cfg.read_config()
        self.assertIn("deepseek", config["profiles"])
        self.assertIn("codex-imported", config["profiles"])
        self.assertEqual(len(config["profiles"]), 2)
        output = captured.getvalue()
        self.assertIn("Config saved", output)


if __name__ == "__main__":
    unittest.main()
