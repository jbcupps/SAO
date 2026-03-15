import io
import sys
import types
import unittest
from pathlib import Path
from unittest.mock import patch

SRC_ROOT = Path(__file__).resolve().parents[1] / "src"
if str(SRC_ROOT) not in sys.path:
    sys.path.insert(0, str(SRC_ROOT))

if "anthropic" not in sys.modules:
    anthropic_stub = types.ModuleType("anthropic")

    class StubAnthropic:
        def __init__(self, api_key: str):
            self.messages = types.SimpleNamespace(create=lambda **kwargs: None)

    anthropic_stub.Anthropic = StubAnthropic
    sys.modules["anthropic"] = anthropic_stub

import main


class MainTests(unittest.TestCase):
    def test_parse_args_supports_cleanup_flag(self):
        args = main.parse_args(["--cleanup", "--resource-group", "sao-rg"])

        self.assertTrue(args.cleanup)
        self.assertEqual(args.resource_group, "sao-rg")

    def test_main_cleanup_mode_skips_api_key_collection(self):
        cleanup_agent = types.SimpleNamespace(
            run_cleanup_mode=lambda resource_group: resource_group == "sao-rg"
        )

        with patch("main.InstallerAgent", return_value=cleanup_agent), patch(
            "main.collect_api_key"
        ) as collect_api_key_mock, patch(
            "sys.stdout", new=io.StringIO()
        ):
            main.main(["--cleanup", "--resource-group", "sao-rg"])

        collect_api_key_mock.assert_not_called()

    def test_parse_args_supports_cleanup_alias_mode(self):
        args = main.parse_args(["uninstall", "--resource-group", "sao-rg"])

        self.assertEqual(args.mode, "uninstall")
        self.assertEqual(args.resource_group, "sao-rg")


if __name__ == "__main__":
    unittest.main()
