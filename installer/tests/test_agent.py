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
    REVIEW_CHECKINS,
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
        self.assertIn(
            "Next I need to run a read-only discovery batch",
            output,
        )
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
            "tools.azure.validate_infrastructure_provisioning",
            return_value='{"status":"Valid"}',
        ), patch(
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
            return_value=(
                'Endpoint: https://sao.example.com\n'
                'Ready: true\n'
                'Runtime state: ready\n'
                'Health: {"status":"ok"}'
            ),
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
        self.assertIn("PostgreSQL admin credential", output)
        self.assertNotIn("No passwords were created", output)

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
            "tools.azure.validate_infrastructure_provisioning",
            return_value='{"status":"Valid"}',
        ), patch(
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
            return_value=(
                'Endpoint: https://sao.example.com\n'
                'Ready: true\n'
                'Runtime state: ready\n'
                'Health: {"status":"ok"}'
            ),
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

    def test_provisioning_polling_treats_runtime_failure_after_arm_success_as_failure(self):
        agent = self._make_agent([])
        status_sequence = iter(
            [
                '{"state":"Running","timestamp":"2026-03-15T12:00:00Z"}',
                '{"state":"Succeeded","timestamp":"2026-03-15T12:00:30Z"}',
            ]
        )

        with patch(
            "tools.azure.validate_infrastructure_provisioning",
            return_value='{"status":"Valid"}',
        ), patch(
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
            return_value=(
                "Endpoint: https://sao.example.com\n"
                "Ready: false\n"
                "Runtime state: failed\n"
                "Revision: sao-app--rev1\n"
                "Revision health: Unhealthy\n"
                "Revision details: Container crashing: sao\n"
                "Application logs:\n"
                "- TLS upgrade required by connect options but SQLx was built without TLS support enabled"
            ),
        ), patch.object(
            agent,
            "_collect_failure_bundle",
            return_value={
                "issue_type": "containerapp_postgres_tls_mismatch",
                "diagnosis": (
                    "The deployed SAO image requires a TLS PostgreSQL connection, "
                    "but SQLx TLS support is missing from the binary."
                ),
                "failed_resource": {
                    "type": "Microsoft.App/containerApps",
                    "name": "sao-app",
                },
                "evidence": [
                    "TLS upgrade required by connect options but SQLx was built without TLS support enabled"
                ],
                "suggested_actions": ["retry_with_image_override"],
            },
        ), patch("time.sleep"), patch(
            "builtins.input", return_value="   "
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
        self.assertIn("Azure runtime verification", result)
        self.assertIn("Likely issue:", result)
        self.assertIn("Recommended fix: rebuild the SAO application image", result)

    def test_provisioning_polling_stops_on_failed_deployment(self):
        agent = self._make_agent([])

        with patch(
            "tools.azure.validate_infrastructure_provisioning",
            return_value='{"status":"Valid"}',
        ), patch(
            "tools.azure.start_infrastructure_provisioning",
            return_value="",
        ), patch(
            "tools.azure.get_group_deployment_status",
            return_value='{"state":"Failed","timestamp":"2026-03-15T12:05:00Z"}',
        ), patch.object(
            agent,
            "_collect_failure_bundle",
            return_value={
                "issue_type": "keyvault_name_conflict",
                "diagnosis": (
                    "The deployment hit a Key Vault global name conflict. "
                    "Azure will not allow two vaults anywhere to share the same DNS name."
                ),
                "failed_resource": {
                    "type": "Microsoft.KeyVault/vaults",
                    "name": "sao-abc-kv",
                },
                "evidence": [
                    "Conflict: Vault name 'sao-abc-kv' is already in use."
                ],
                "suggested_actions": [
                    "retry_with_name_suffix",
                    "cleanup_resource_group",
                ],
            },
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
        self.assertIn("Likely issue:", result)
        self.assertIn("Failing resource:", result)
        self.assertIn("Suggested recovery:", result)

    def test_review_last_failure_returns_cached_bundle(self):
        agent = self._make_agent([])
        agent.installer_state["last_failure_bundle"] = {
            "issue_type": "container_image_denied",
            "diagnosis": "Azure could not pull the configured image.",
            "failed_resource": {
                "type": "Microsoft.App/containerApps",
                "name": "sao-app",
            },
        }
        agent.installer_state["troubleshooting_active"] = True

        result = agent._review_last_failure(host_os="windows")

        self.assertIn('"issue_type": "container_image_denied"', result)
        self.assertIn('"name": "sao-app"', result)

    def test_apply_guided_fix_purges_deleted_key_vault_and_retries(self):
        agent = self._make_agent([])
        agent.installer_state["resource_group"] = "sao-rg"
        agent.installer_state["location"] = "eastus2"
        agent.installer_state["admin_oid"] = "oid-123"
        agent.installer_state["last_failure_bundle"] = {
            "deleted_vault": {"name": "sao-abc-kv", "location": "eastus2"},
            "failed_resource_name": "sao-abc-kv",
        }

        with patch(
            "tools.azure.purge_deleted_key_vault",
            return_value="Purge requested for deleted Key Vault sao-abc-kv in eastus2.",
        ) as purge_mock, patch.object(
            agent,
            "_run_provisioning_with_polling",
            return_value='{"provisioning_state":"Succeeded"}',
        ) as retry_mock:
            result = agent._apply_guided_fix(
                {"action": "purge_deleted_key_vault"},
                host_os="windows",
            )

        purge_mock.assert_called_once_with(
            "sao-abc-kv", "eastus2", host_os="windows"
        )
        retry_mock.assert_called_once()
        self.assertIn("Retry result:", result)
        self.assertIn('"provisioning_state":"Succeeded"', result)

    def test_apply_guided_fix_retry_with_name_suffix_sets_suffix(self):
        agent = self._make_agent([])
        agent.installer_state["resource_group"] = "sao-rg"
        agent.installer_state["location"] = "eastus2"
        agent.installer_state["admin_oid"] = "oid-123"

        with patch.object(
            agent,
            "_run_provisioning_with_polling",
            return_value='{"provisioning_state":"Succeeded","name_suffix":"a7c"}',
        ) as retry_mock:
            result = agent._apply_guided_fix(
                {"action": "retry_with_name_suffix", "name_suffix": "a7c"},
                host_os="windows",
            )

        self.assertEqual(agent.installer_state["name_suffix"], "a7c")
        retry_mock.assert_called_once()
        self.assertIn("unique suffix a7c", result)

    def test_apply_guided_fix_retry_with_image_override_sets_override(self):
        agent = self._make_agent([])
        agent.installer_state["resource_group"] = "sao-rg"
        agent.installer_state["location"] = "eastus2"
        agent.installer_state["admin_oid"] = "oid-123"

        with patch.object(
            agent,
            "_run_provisioning_with_polling",
            return_value='{"provisioning_state":"Succeeded","image_reference":"ghcr.io/example/sao:v2"}',
        ) as retry_mock:
            result = agent._apply_guided_fix(
                {
                    "action": "retry_with_image_override",
                    "image_reference": "ghcr.io/example/sao:v2",
                },
                host_os="windows",
            )

        self.assertEqual(
            agent.installer_state["image_override"],
            "ghcr.io/example/sao:v2",
        )
        retry_mock.assert_called_once()
        self.assertIn("image override ghcr.io/example/sao:v2", result)

    def test_summary_response_accepts_varied_checkins(self):
        agent = self._make_agent([])

        for checkin in REVIEW_CHECKINS:
            self.assertTrue(
                agent._summary_response_is_valid(
                    [FakeBlock("text", text=f"Summary. {checkin}")]
                )
            )

    def test_runtime_system_prompt_includes_troubleshooting_context(self):
        agent = self._make_agent([])
        agent.installer_state["troubleshooting_active"] = True
        agent.installer_state["last_failure_bundle"] = {
            "issue_type": "container_image_denied",
            "diagnosis": "Azure could not pull the image.",
            "failed_resource": {
                "type": "Microsoft.App/containerApps",
                "name": "sao-app",
            },
            "suggested_actions": ["retry_with_image_override"],
            "evidence": ["DENIED: requested access to the resource is denied"],
        }

        prompt = agent._build_runtime_system_prompt()

        self.assertIn("Troubleshooting mode is active", prompt)
        self.assertIn("review_last_failure", prompt)
        self.assertIn("apply_guided_fix", prompt)
        self.assertIn("container_image_denied", prompt)

    def test_format_provisioning_failure_message_calls_out_ghcr_visibility(self):
        agent = self._make_agent([])

        result = agent._format_provisioning_failure_message(
            deployment_name="sao-bootstrap",
            context_label="Provisioning start",
            diagnostics={
                "issue_type": "container_image_ghcr_private",
                "diagnosis": (
                    "The Container App reached GitHub Container Registry, but "
                    "GHCR denied anonymous access to the configured image."
                ),
                "image_reference": "ghcr.io/jbcupps/sao:latest",
                "failed_resource": {
                    "type": "Microsoft.App/containerApps",
                    "name": "sao-app",
                },
                "suggested_actions": ["retry_with_image_override"],
            },
        )

        self.assertIn(
            "set the GitHub Container Registry package backing "
            "ghcr.io/jbcupps/sao:latest to Public",
            result,
        )
        self.assertIn("Make the GHCR package public in GitHub", result)

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
                f"\n{agent._checkin_for_phase('cleanup')}\nYou: ",
                "Would you like fresh-install instructions now? (y/n) ",
            ],
        )
        output = stdout.getvalue()
        self.assertIn("Let me quickly confirm the cleanup plan:", output)
        self.assertIn("Target resource group: sao-rg", output)
        self.assertIn("Cleanup summary:", output)
        self.assertIn(
            "Re-run this bootstrapper without --cleanup to start a fresh install.",
            output,
        )


if __name__ == "__main__":
    unittest.main()
