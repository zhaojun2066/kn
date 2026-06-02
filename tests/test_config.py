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


if __name__ == "__main__":
    unittest.main()
