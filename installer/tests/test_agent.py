import copy
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

from agent import (
    CONTINUE_MESSAGE,
    DEFAULT_DEPLOYMENT_NAME,
    InstallerAgent,
    POLL_INTERVAL_SECONDS,
    REQUIRED_CHECKIN,
)


class FakeBlock:
    def __init__(
        self,
        block_type: str,
        text: str | None = None,
        name: str | None = None,
        block_input: dict | None = None,
        block_id: str | None = None,
    ):
        self.type = block_type
        self.text = text
        self.name = name
        self.input = block_input or {}
        self.id = block_id or "block-id"


class FakeResponse:
    def __init__(self, content, stop_reason: str = "end_turn"):
        self.content = content
        self.stop_reason = stop_reason


class FakeMessages:
    def __init__(self, responses):
        self._responses = iter(responses)
        self.calls = []

    def create(self, **kwargs):
        self.calls.append(copy.deepcopy(kwargs))
        return next(self._responses)


class FakeClient:
    def __init__(self, responses):
        self.messages = FakeMessages(responses)


class InstallerAgentTests(unittest.TestCase):
    def _make_agent(self, responses):
        agent = InstallerAgent(provider="claude", api_key="sk-ant-test")
        agent.client = FakeClient(responses)
        return agent

    def test_read_only_tools_share_one_batch_approval(self):
        responses = [
            FakeResponse(
                [
                    FakeBlock("text", text="Starting discovery."),
                    FakeBlock(
                        "tool_use",
                        name="get_signed_in_user",
                        block_input={},
                        block_id="tool-1",
                    ),
                    FakeBlock(
                        "tool_use",
                        name="check_permissions",
                        block_input={},
                        block_id="tool-2",
                    ),
                ]
            ),
            FakeResponse(
                [
                    FakeBlock(
                        "text",
                        text=(
                            "I confirmed your identity and checked the Azure "
                            f"prerequisites. {REQUIRED_CHECKIN}"
                        ),
                    )
                ]
            ),
        ]
        prompts: list[str] = []
        answers = iter(["y", "quit"])
        stdout = io.StringIO()

        def fake_input(prompt: str) -> str:
            prompts.append(prompt)
            return next(answers)

        agent = self._make_agent(responses)

        with patch.object(
            agent,
            "_dispatch_tool",
            side_effect=[
                '{"oid":"oid-123","upn":"user@example.com"}',
                '{"graph_api":"OK"}',
            ],
        ) as dispatch_mock, patch(
            "builtins.input", side_effect=fake_input
        ), patch("sys.stdout", stdout):
            agent.run()

        approval_prompts = [prompt for prompt in prompts if prompt.startswith("Approve")]
        self.assertEqual(approval_prompts, ["Approve this read-only batch? (y/n) "])
        self.assertEqual(dispatch_mock.call_count, 2)
        output = stdout.getvalue()
        self.assertIn("Here's what I'm about to do and why:", output)
        self.assertIn(REQUIRED_CHECKIN, output)

    def test_subscription_and_provisioning_require_separate_approvals(self):
        responses = [
            FakeResponse(
                [
                    FakeBlock(
                        "tool_use",
                        name="set_subscription",
                        block_input={"subscription_id": "sub-123"},
                        block_id="tool-1",
                    )
                ]
            ),
            FakeResponse(
                [
                    FakeBlock(
                        "text",
                        text=f"Active subscription updated. {REQUIRED_CHECKIN}",
                    )
                ]
            ),
            FakeResponse(
                [
                    FakeBlock(
                        "tool_use",
                        name="provision_infrastructure",
                        block_input={
                            "resource_group": "sao-rg",
                            "location": "eastus2",
                            "admin_oid": "oid-123",
                        },
                        block_id="tool-2",
                    )
                ]
            ),
            FakeResponse(
                [
                    FakeBlock(
                        "text",
                        text=f"Provisioning completed. {REQUIRED_CHECKIN}",
                    )
                ]
            ),
        ]
        prompts: list[str] = []
        answers = iter(["y", "continue", "y", "quit"])

        def fake_input(prompt: str) -> str:
            prompts.append(prompt)
            return next(answers)

        agent = self._make_agent(responses)

        with patch.object(
            agent,
            "_dispatch_tool",
            side_effect=["Subscription set"],
        ) as dispatch_mock, patch.object(
            agent,
            "_run_provisioning_with_polling",
            return_value='{"provisioning_state":"Succeeded"}',
        ) as provisioning_mock, patch(
            "builtins.input", side_effect=fake_input
        ), patch("sys.stdout", new=io.StringIO()):
            agent.run()

        approval_prompts = [prompt for prompt in prompts if prompt.startswith("Approve")]
        self.assertEqual(
            approval_prompts,
            [
                "Approve active subscription change? (y/n) ",
                "Approve SAO infrastructure deployment? (y/n) ",
            ],
        )
        self.assertEqual(dispatch_mock.call_count, 1)
        provisioning_mock.assert_called_once()

    def test_provisioning_polling_answers_questions_and_completes_handoff(self):
        agent = self._make_agent(
            [
                FakeResponse(
                    [
                        FakeBlock(
                            "text",
                            text="Azure is still creating the managed database.",
                        )
                    ]
                )
            ]
        )
        prompts: list[str] = []
        answers = iter(["status", "What is being provisioned right now?"])
        stdout = io.StringIO()

        def fake_input(prompt: str) -> str:
            prompts.append(prompt)
            return next(answers)

        status_sequence = iter(
            [
                '{"state":"Running","timestamp":"2026-03-15T12:00:00Z"}',
                '{"state":"Running","timestamp":"2026-03-15T12:00:30Z"}',
                '{"state":"Succeeded","timestamp":"2026-03-15T12:01:00Z"}',
            ]
        )

        with patch(
            "tools.azure.start_infrastructure_provisioning",
            return_value="",
        ), patch(
            "tools.azure.get_group_deployment_status",
            side_effect=lambda **kwargs: next(status_sequence),
        ), patch(
            "tools.azure.list_resource_group_resource_types",
            return_value="[]",
        ), patch(
            "tools.azure.get_group_deployment_endpoint",
            return_value="sao.example.com",
        ), patch(
            "tools.azure.check_deployment_status",
            return_value='Endpoint: https://sao.example.com\nHealth: {"status":"ok"}',
        ), patch(
            "time.sleep"
        ) as sleep_mock, patch(
            "builtins.input", side_effect=fake_input
        ), patch("sys.stdout", new=stdout):
            result = agent._run_provisioning_with_polling(
                {
                    "resource_group": "sao-rg",
                    "location": "eastus2",
                    "admin_oid": "oid-123",
                },
                host_os="windows",
            )

        self.assertIn('"provisioning_state": "Succeeded"', result)
        self.assertIn('"endpoint": "https://sao.example.com"', result)
        self.assertEqual(
            prompts,
            [
                "\nDuring provisioning, press Enter to keep waiting, type 'status' to refresh now, or ask a question: ",
                "\nDuring provisioning, press Enter to keep waiting, type 'status' to refresh now, or ask a question: ",
            ],
        )
        sleep_mock.assert_called_once_with(POLL_INTERVAL_SECONDS)
        question_call = agent.client.messages.calls[0]
        self.assertIn(
            "What is being provisioned right now?",
            question_call["messages"][0]["content"],
        )
        self.assertIn(
            DEFAULT_DEPLOYMENT_NAME,
            question_call["messages"][0]["content"],
        )
        output = stdout.getvalue()
        self.assertIn("Deployment is still running", output)
        self.assertIn("Provisioning PostgreSQL.", output)
        self.assertIn("Provisioning complete.", output)
        self.assertIn("SAO endpoint: https://sao.example.com", output)
        self.assertIn("Bootstrap admin OID: oid-123", output)

    def test_provisioning_polling_blank_input_keeps_waiting_without_model_call(self):
        agent = self._make_agent([])
        answers = iter(["   "])
        status_sequence = iter(
            [
                '{"state":"Running","timestamp":"2026-03-15T12:00:00Z"}',
                '{"state":"Succeeded","timestamp":"2026-03-15T12:00:30Z"}',
            ]
        )

        with patch(
            "tools.azure.start_infrastructure_provisioning",
            return_value="",
        ), patch(
            "tools.azure.get_group_deployment_status",
            side_effect=lambda **kwargs: next(status_sequence),
        ), patch(
            "tools.azure.list_resource_group_resource_types",
            return_value='["Microsoft.DBforPostgreSQL/flexibleServers"]',
        ), patch(
            "tools.azure.get_group_deployment_endpoint",
            return_value="sao.example.com",
        ), patch(
            "tools.azure.check_deployment_status",
            return_value='Endpoint: https://sao.example.com\nHealth: {"status":"ok"}',
        ), patch(
            "time.sleep"
        ) as sleep_mock, patch(
            "builtins.input", side_effect=lambda prompt: next(answers)
        ), patch("sys.stdout", new=io.StringIO()):
            result = agent._run_provisioning_with_polling(
                {
                    "resource_group": "sao-rg",
                    "location": "eastus2",
                    "admin_oid": "oid-123",
                },
                host_os="windows",
            )

        self.assertIn('"provisioning_state": "Succeeded"', result)
        self.assertEqual(agent.client.messages.calls, [])
        sleep_mock.assert_called_once_with(POLL_INTERVAL_SECONDS)

    def test_provisioning_polling_stops_on_failed_deployment(self):
        agent = self._make_agent([])

        with patch(
            "tools.azure.start_infrastructure_provisioning",
            return_value="",
        ), patch(
            "tools.azure.get_group_deployment_status",
            return_value='{"state":"Failed","timestamp":"2026-03-15T12:05:00Z"}',
        ), patch("sys.stdout", new=io.StringIO()):
            result = agent._run_provisioning_with_polling(
                {
                    "resource_group": "sao-rg",
                    "location": "eastus2",
                    "admin_oid": "oid-123",
                },
                host_os="windows",
            )

        self.assertIn("COMMAND FAILED", result)
        self.assertIn("state Failed", result)

    def test_declined_write_step_does_not_execute_tool(self):
        responses = [
            FakeResponse(
                [
                    FakeBlock(
                        "tool_use",
                        name="create_resource_group",
                        block_input={"name": "sao-rg", "location": "eastus2"},
                        block_id="tool-1",
                    )
                ]
            ),
            FakeResponse(
                [
                    FakeBlock(
                        "text",
                        text=(
                            "The resource group was not created because the step "
                            f"was declined. {REQUIRED_CHECKIN}"
                        ),
                    )
                ]
            ),
        ]
        prompts: list[str] = []
        answers = iter(["n", "quit"])

        def fake_input(prompt: str) -> str:
            prompts.append(prompt)
            return next(answers)

        agent = self._make_agent(responses)

        with patch.object(agent, "_dispatch_tool") as dispatch_mock, patch(
            "builtins.input", side_effect=fake_input
        ), patch("sys.stdout", new=io.StringIO()):
            agent.run()

        approval_prompts = [prompt for prompt in prompts if prompt.startswith("Approve")]
        self.assertEqual(
            approval_prompts,
            ["Approve resource group creation? (y/n) "],
        )
        dispatch_mock.assert_not_called()

    def test_conversational_cleanup_requires_explicit_approval(self):
        responses = [
            FakeResponse(
                [
                    FakeBlock(
                        "tool_use",
                        name="delete_resource_group",
                        block_input={"resource_group": "sao-rg"},
                        block_id="tool-1",
                    )
                ]
            ),
            FakeResponse(
                [
                    FakeBlock(
                        "text",
                        text=f"Cleanup request submitted. {REQUIRED_CHECKIN}",
                    )
                ]
            ),
        ]
        prompts: list[str] = []
        answers = iter(["y", "quit"])

        def fake_input(prompt: str) -> str:
            prompts.append(prompt)
            return next(answers)

        agent = self._make_agent(responses)

        with patch.object(
            agent,
            "_dispatch_tool",
            return_value=(
                "Cleanup requested for resource group sao-rg. Azure will "
                "remove the SAO test deployment and every child resource "
                "inside that group."
            ),
        ) as dispatch_mock, patch(
            "builtins.input", side_effect=fake_input
        ), patch("sys.stdout", new=io.StringIO()):
            agent.run()

        approval_prompts = [prompt for prompt in prompts if prompt.startswith("Approve")]
        self.assertEqual(
            approval_prompts,
            ["Approve resource group cleanup? (y/n) "],
        )
        dispatch_mock.assert_called_once()

    def test_blank_user_reply_becomes_non_empty_continue_message(self):
        responses = [
            FakeResponse(
                [
                    FakeBlock(
                        "text",
                        text=f"Identity confirmed. {REQUIRED_CHECKIN}",
                    )
                ]
            ),
            FakeResponse(
                [
                    FakeBlock(
                        "text",
                        text="Proceeding to the next step.",
                    )
                ]
            ),
        ]
        answers = iter(["   ", "quit"])

        def fake_input(prompt: str) -> str:
            return next(answers)

        agent = self._make_agent(responses)

        with patch("builtins.input", side_effect=fake_input), patch(
            "sys.stdout", new=io.StringIO()
        ):
            agent.run()

        second_call_messages = agent.client.messages.calls[1]["messages"]
        self.assertEqual(
            second_call_messages[-1],
            {"role": "user", "content": CONTINUE_MESSAGE},
        )

    def test_run_cleanup_mode_narrates_and_offers_fresh_install(self):
        prompts: list[str] = []
        answers = iter(["y", "", "y"])
        stdout = io.StringIO()
        agent = InstallerAgent(provider="cleanup", api_key=None)

        def fake_input(prompt: str) -> str:
            prompts.append(prompt)
            return next(answers)

        with patch.object(
            agent,
            "_dispatch_tool",
            return_value=(
                "Cleanup requested for resource group sao-rg. Azure will "
                "remove the SAO test deployment and every child resource "
                "inside that group."
            ),
        ) as dispatch_mock, patch(
            "builtins.input", side_effect=fake_input
        ), patch("sys.stdout", new=stdout):
            success = agent.run_cleanup_mode("sao-rg")

        self.assertTrue(success)
        dispatch_mock.assert_called_once_with(
            "delete_resource_group", {"resource_group": "sao-rg"}
        )
        self.assertEqual(
            prompts,
            [
                "Approve cleanup of resource group sao-rg? (y/n) ",
                f"\n{REQUIRED_CHECKIN}\nYou: ",
                "Would you like fresh-install instructions now? (y/n) ",
            ],
        )
        output = stdout.getvalue()
        self.assertIn("Here's what I'm about to do and why:", output)
        self.assertIn("Target resource group: sao-rg", output)
        self.assertIn("Cleanup summary:", output)
        self.assertIn(
            "Re-run this bootstrapper without --cleanup to start a fresh install.",
            output,
        )


if __name__ == "__main__":
    unittest.main()
