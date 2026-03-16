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
    def test_masked_secret_reader_echoes_stars_and_hides_raw_secret(self):
        output = io.StringIO()
        characters = iter(["s", "k", "-", "1", "2", "\x08", "3", "\r"])

        secret = main._read_masked_secret_from_reader(
            "Enter key: ",
            lambda: next(characters),
            stdout=output,
        )

        self.assertEqual(secret, "sk-13")
        rendered = output.getvalue()
        self.assertIn("Enter key: ", rendered)
        self.assertIn("*", rendered)
        self.assertNotIn("sk-13", rendered)

    def test_read_masked_secret_uses_plain_readline_when_stdin_is_not_a_tty(self):
        class FakeInput(io.StringIO):
            def isatty(self) -> bool:
                return False

        output = io.StringIO()
        secret = main.read_masked_secret(
            "Enter key: ",
            stdin=FakeInput("sk-ant-test\n"),
            stdout=output,
        )

        self.assertEqual(secret, "sk-ant-test")
        self.assertEqual(output.getvalue(), "Enter key: \n")

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
