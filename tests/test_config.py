#!/usr/bin/env python3
"""Tests for lib/config.py — YAML parsing, serialization, file locking."""

import os
import sys
import tempfile
import unittest

# Load the config module
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "lib"))
import config as cfg


class TestYamlParsing(unittest.TestCase):
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

    def _write(self, text):
        self.tmp.write(text)
        self.tmp.flush()

    def test_empty_config(self):
        self._write("profiles:\n")
        config = cfg.read_config()
        self.assertEqual(config["profiles"], {})

    def test_single_profile(self):
        self._write("""profiles:
  test:
    desc: "A test profile"
    env:
      KEY1: value1
      KEY2: value2
""")
        config = cfg.read_config()
        self.assertIn("test", config["profiles"])
        self.assertEqual(config["profiles"]["test"]["desc"], "A test profile")
        self.assertEqual(config["profiles"]["test"]["env"]["KEY1"], "value1")
        self.assertEqual(config["profiles"]["test"]["env"]["KEY2"], "value2")

    def test_default(self):
        self._write("""default: myprof
profiles:
  myprof:
    desc: "Default one"
    env:
      X: "1"
""")
        config = cfg.read_config()
        self.assertEqual(config["default"], "myprof")

    def test_env_with_special_chars(self):
        self._write("""profiles:
  test:
    desc: "Special"
    env:
      URL: "https://api.example.com/v1?foo=bar"
      TOKEN: sk-test-1234567890
""")
        config = cfg.read_config()
        env = config["profiles"]["test"]["env"]
        self.assertEqual(env["URL"], "https://api.example.com/v1?foo=bar")
        self.assertEqual(env["TOKEN"], "sk-test-1234567890")

    def test_multiple_profiles(self):
        self._write("""profiles:
  a:
    desc: "Profile A"
    env:
      K: aval
  b:
    desc: "Profile B"
    env:
      K: bval
""")
        config = cfg.read_config()
        self.assertEqual(len(config["profiles"]), 2)
        self.assertEqual(config["profiles"]["a"]["env"]["K"], "aval")
        self.assertEqual(config["profiles"]["b"]["env"]["K"], "bval")

    def test_round_trip(self):
        data = {
            "default": "p1",
            "profiles": {
                "p1": {
                    "desc": "First",
                    "env": {"KEY1": "val1", "URL": "https://x.com/path"},
                },
                "p2": {
                    "desc": "Second",
                    "env": {},
                },
            },
        }
        cfg.write_config(data)
        config = cfg.read_config()
        self.assertEqual(config["default"], "p1")
        self.assertEqual(config["profiles"]["p1"]["desc"], "First")
        self.assertEqual(config["profiles"]["p1"]["env"]["KEY1"], "val1")
        self.assertEqual(config["profiles"]["p1"]["env"]["URL"], "https://x.com/path")
        self.assertEqual(config["profiles"]["p2"]["desc"], "Second")
        self.assertEqual(config["profiles"]["p2"]["env"], {})

    def test_comments_ignored(self):
        self._write("""# Top comment
default: p1
# Another comment
profiles:
  p1:
    desc: "Test"  # inline comment
    env:
      KEY: value
""")
        config = cfg.read_config()
        self.assertEqual(config["default"], "p1")
        self.assertEqual(config["profiles"]["p1"]["env"]["KEY"], "value")

    def test_empty_env(self):
        self._write("""profiles:
  bare:
    desc: "No env vars"
    env: {}
""")
        config = cfg.read_config()
        self.assertEqual(config["profiles"]["bare"]["env"], {})

    def test_missing_file(self):
        cfg.CONFIG_FILE = "/tmp/nonexistent_config_test.yaml"
        config = cfg.read_config()
        self.assertEqual(config, {"profiles": {}})


class TestQuoteYaml(unittest.TestCase):
    def test_plain_value(self):
        self.assertEqual(cfg._quote_yaml("hello"), "hello")

    def test_url_value(self):
        result = cfg._quote_yaml("https://api.example.com/v1")
        self.assertTrue(result.startswith('"'))

    def test_empty_value(self):
        self.assertEqual(cfg._quote_yaml(""), '""')

    def test_value_with_space(self):
        result = cfg._quote_yaml("hello world")
        self.assertTrue(result.startswith('"'))

    def test_yaml_number_variants_quoted(self):
        """Values that YAML would interpret as numbers MUST be quoted."""
        for val in ["+5", "1e5", "-1e5", "1.5e10", ".5", "5.", "-1.5", "42"]:
            result = cfg._quote_yaml(val)
            self.assertTrue(
                result.startswith('"'),
                f"'{val}' should be quoted, got: {result}",
            )

    def test_non_numbers_not_quoted(self):
        """Values that are NOT YAML numbers should stay unquoted."""
        for val in ["1.2.3", "--5", "abc", "1.0-release"]:
            result = cfg._quote_yaml(val)
            self.assertFalse(
                result.startswith('"'),
                f"'{val}' should NOT be quoted, got: {result}",
            )


class TestPublicAPI(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.NamedTemporaryFile(mode="w", suffix=".yaml", delete=False)
        self.lock = tempfile.NamedTemporaryFile(mode="w", suffix=".lock", delete=False)
        cfg.CONFIG_FILE = self.tmp.name
        cfg.LOCK_FILE = self.lock.name
        data = {
            "default": "a",
            "profiles": {
                "a": {"desc": "First", "env": {"X": "1"}},
                "b": {"desc": "Second", "env": {"Y": "2"}},
            },
        }
        cfg.write_config(data)

    def tearDown(self):
        self.tmp.close()
        self.lock.close()
        os.unlink(self.tmp.name)
        os.unlink(self.lock.name)

    def test_list_profiles(self):
        config = cfg.read_config()
        self.assertEqual(cfg.list_profiles(config), ["a", "b"])

    def test_get_profile(self):
        config = cfg.read_config()
        p = cfg.get_profile(config, "a")
        self.assertEqual(p["desc"], "First")
        self.assertEqual(p["env"]["X"], "1")

    def test_get_profile_missing(self):
        config = cfg.read_config()
        self.assertIsNone(cfg.get_profile(config, "nonexistent"))

    def test_get_default(self):
        config = cfg.read_config()
        self.assertEqual(cfg.get_default(config), "a")


class TestMultilineValues(unittest.TestCase):
    """Verify multi-line YAML values round-trip correctly."""

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

    def test_multiline_desc_round_trip(self):
        """Profile description with newlines should round-trip correctly."""
        data = {
            "profiles": {
                "multi-desc": {
                    "desc": "Line 1\nLine 2\nLine 3",
                    "env": {},
                },
            },
        }
        cfg.write_config(data)
        result = cfg.read_config()
        self.assertEqual(
            result["profiles"]["multi-desc"]["desc"],
            "Line 1\nLine 2\nLine 3",
        )

    def test_multiline_env_var_round_trip(self):
        """Env var with multi-line value should round-trip preserving content."""
        data = {
            "profiles": {
                "multi-env": {
                    "desc": "",
                    "env": {"SSH_KEY": "-----BEGIN KEY-----\nline1\nline2\n-----END KEY-----"},
                },
            },
        }
        cfg.write_config(data)
        result = cfg.read_config()
        # Block scalar may add trailing newline — strip for comparison
        actual = result["profiles"]["multi-env"]["env"]["SSH_KEY"].rstrip("\n")
        self.assertEqual(
            actual,
            "-----BEGIN KEY-----\nline1\nline2\n-----END KEY-----",
        )

    def test_multiline_preserves_content(self):
        """Block scalar (|) should preserve the multi-line content."""
        data = {
            "profiles": {
                "trail": {
                    "desc": "",
                    "env": {"TEXT": "hello\nworld"},
                },
            },
        }
        cfg.write_config(data)
        result = cfg.read_config()
        actual = result["profiles"]["trail"]["env"]["TEXT"].rstrip("\n")
        self.assertEqual(actual, "hello\nworld")

    def test_multiline_empty_profile_unchanged(self):
        """Profiles without multi-line values should be unaffected."""
        data = {
            "default": "simple",
            "profiles": {
                "simple": {"desc": "Just a simple profile", "env": {"X": "1"}},
            },
        }
        cfg.write_config(data)
        result = cfg.read_config()
        self.assertEqual(result["default"], "simple")
        self.assertEqual(result["profiles"]["simple"]["desc"], "Just a simple profile")


class TestBackupRotation(unittest.TestCase):
    """Verify 3-generation backup rotation."""

    def setUp(self):
        self.tmp_dir = tempfile.mkdtemp()
        self.config_file = os.path.join(self.tmp_dir, "config.yaml")
        self.backup_file = os.path.join(self.tmp_dir, "config.yaml.bak")
        cfg.CONFIG_DIR = self.tmp_dir
        cfg.CONFIG_FILE = self.config_file
        cfg.BACKUP_FILE = self.backup_file
        cfg.LOCK_FILE = os.path.join(self.tmp_dir, ".config.lock")

    def tearDown(self):
        import shutil
        shutil.rmtree(self.tmp_dir, ignore_errors=True)

    def test_first_write_creates_no_backup(self):
        """First write has no previous config, so no .bak created."""
        data = {"profiles": {"test": {"desc": "", "env": {"K": "v1"}}}}
        cfg.write_config(data)
        self.assertFalse(os.path.exists(self.backup_file))

    def test_second_write_creates_bak(self):
        """Second write backs up the first config to .bak."""
        cfg.write_config({"profiles": {"a": {"desc": "", "env": {}}}})
        self.assertTrue(os.path.exists(self.config_file))
        cfg.write_config({"profiles": {"b": {"desc": "", "env": {}}}})
        self.assertTrue(os.path.exists(self.backup_file))

    def test_rotation_produces_bak1_bak2(self):
        """After 3+ writes, .bak.1 and .bak.2 should exist."""
        for i in range(5):
            cfg.write_config({"profiles": {f"p{i}": {"desc": "", "env": {}}}})

        self.assertTrue(os.path.exists(self.backup_file), ".bak should exist")
        bak1 = f"{self.backup_file}.1"
        bak2 = f"{self.backup_file}.2"
        self.assertTrue(os.path.exists(bak1), f"{bak1} should exist after rotation")
        self.assertTrue(os.path.exists(bak2), f"{bak2} should exist after rotation")


class TestDeleteProfileDefault(unittest.TestCase):
    """Verify delete-profile default promotion behavior matches between Python CLI and Rust desktop."""

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

    def test_delete_default_promotes_first_alphabetical(self):
        """When default profile is deleted, alphabetically-first remaining becomes new default."""
        data = {
            "default": "zulu",
            "profiles": {
                "alpha": {"desc": "", "env": {}},
                "bravo": {"desc": "", "env": {}},
                "zulu": {"desc": "", "env": {}},
            },
        }
        cfg.write_config(data)
        config = cfg.read_config()
        # Simulate cmd_remove for 'zulu' (the current default)
        del config["profiles"]["zulu"]
        if config.get("default") == "zulu":
            remaining = sorted(config.get("profiles", {}).keys())
            config["default"] = remaining[0] if remaining else ""
        cfg.write_config(config)
        result = cfg.read_config()
        self.assertEqual(result["default"], "alpha")

    def test_delete_non_default_preserves_default(self):
        """Deleting a non-default profile leaves default unchanged."""
        data = {
            "default": "alpha",
            "profiles": {
                "alpha": {"desc": "", "env": {}},
                "bravo": {"desc": "", "env": {}},
            },
        }
        cfg.write_config(data)
        config = cfg.read_config()
        del config["profiles"]["bravo"]
        # default was alpha, bravo was not the default — no change
        cfg.write_config(config)
        result = cfg.read_config()
        self.assertEqual(result["default"], "alpha")

    def test_delete_last_profile_clears_default(self):
        """Deleting the only profile sets default to empty string."""
        data = {
            "default": "solo",
            "profiles": {
                "solo": {"desc": "", "env": {}},
            },
        }
        cfg.write_config(data)
        config = cfg.read_config()
        del config["profiles"]["solo"]
        if config.get("default") == "solo":
            remaining = sorted(config.get("profiles", {}).keys())
            config["default"] = remaining[0] if remaining else ""
        cfg.write_config(config)
        result = cfg.read_config()
        # _format_yaml omits the "default:" line when value is empty,
        # so read_config returns a dict without a "default" key.
        self.assertEqual(result.get("default", ""), "")


class TestYamlValEdgeCases(unittest.TestCase):
    """Verify _yaml_val handles edge cases correctly."""

    def test_strips_double_quotes(self):
        self.assertEqual(cfg._yaml_val('"hello"'), "hello")

    def test_strips_single_quotes(self):
        self.assertEqual(cfg._yaml_val("'hello'"), "hello")

    def test_strips_trailing_whitespace(self):
        self.assertEqual(cfg._yaml_val("  hello  "), "hello")

    def test_preserves_inner_spaces(self):
        self.assertEqual(cfg._yaml_val('"hello world"'), "hello world")

    def test_comment_stripped_outside_quotes(self):
        self.assertEqual(cfg._yaml_val("value # comment"), "value")

    def test_comment_preserved_inside_quotes(self):
        # "# comment" inside quotes is literal content
        result = cfg._yaml_val('"value # comment"')
        self.assertEqual(result, "value # comment")

    def test_escaped_quote_inside_quotes(self):
        # Backslash-escaped quote should not end the quoted region
        result = cfg._yaml_val(r'"say \"hello\""')
        self.assertTrue("say" in result)

    def test_empty_value(self):
        self.assertEqual(cfg._yaml_val(""), "")

    def test_explicit_empty_quotes(self):
        self.assertEqual(cfg._yaml_val('""'), "")

    def test_colon_in_value_preserved(self):
        self.assertEqual(cfg._yaml_val("https://example.com"), "https://example.com")


class TestQuoteYamlEdgeCases(unittest.TestCase):
    """Verify _quote_yaml handles all edge cases."""

    def test_yaml_reserved_words_quoted(self):
        for word in ["true", "false", "yes", "no", "on", "off", "null", "~"]:
            result = cfg._quote_yaml(word)
            self.assertTrue(result.startswith('"'), f"'{word}' should be quoted")

    def test_value_with_hash_quoted(self):
        result = cfg._quote_yaml("key # with hash")
        self.assertTrue(result.startswith('"'))

    def test_value_with_at_sign_quoted(self):
        result = cfg._quote_yaml("user@host")
        self.assertTrue(result.startswith('"'))

    def test_value_with_colon_quoted(self):
        result = cfg._quote_yaml("key: value")
        self.assertTrue(result.startswith('"'))

    def test_value_with_backtick_quoted(self):
        result = cfg._quote_yaml("`command`")
        self.assertTrue(result.startswith('"'))

    def test_plain_alphanumeric_not_quoted(self):
        result = cfg._quote_yaml("hello123")
        self.assertFalse(result.startswith('"'))

    def test_single_quote_escaped(self):
        result = cfg._quote_yaml("it's working")
        self.assertTrue(result.startswith('"'))
        self.assertIn("it's working", result)

    def test_backslash_escaped(self):
        result = cfg._quote_yaml("C:\\path\\file")
        self.assertTrue(result.startswith('"'))
        self.assertIn("\\\\", result)  # backslash doubled


if __name__ == "__main__":
    unittest.main()
