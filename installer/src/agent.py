"""SAO Installer — Conversation manager and tool dispatch."""

import json
import os
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import anthropic

TOOLS = [
    {
        "name": "az_login",
        "description": (
            "Initiate Azure device code login flow. Use this only for the "
            "authentication phase."
        ),
        "input_schema": {
            "type": "object",
            "properties": {},
            "required": [],
        },
    },
    {
        "name": "get_signed_in_user",
        "description": (
            "Read the currently signed-in Azure user's OID, UPN, and display "
            "name as part of the read-only discovery phase."
        ),
        "input_schema": {
            "type": "object",
            "properties": {},
            "required": [],
        },
    },
    {
        "name": "list_subscriptions",
        "description": (
            "Read the Azure subscriptions the signed-in user can access as "
            "part of the read-only discovery phase."
        ),
        "input_schema": {
            "type": "object",
            "properties": {},
            "required": [],
        },
    },
    {
        "name": "set_subscription",
        "description": (
            "Set the active Azure subscription. Use only after the user "
            "explicitly confirms the target subscription."
        ),
        "input_schema": {
            "type": "object",
            "properties": {
                "subscription_id": {
                    "type": "string",
                    "description": "The subscription ID to set as active",
                }
            },
            "required": ["subscription_id"],
        },
    },
    {
        "name": "check_permissions",
        "description": (
            "Run the read-only SAO pre-flight permission checks. This verifies "
            "the active subscription, role assignments, Graph API access, and "
            "required resource providers."
        ),
        "input_schema": {
            "type": "object",
            "properties": {},
            "required": [],
        },
    },
    {
        "name": "create_resource_group",
        "description": (
            "Create an Azure resource group for SAO. This is a write action "
            "and should only happen after the user confirms the name and region."
        ),
        "input_schema": {
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Resource group name",
                },
                "location": {
                    "type": "string",
                    "description": "Azure region (e.g., eastus2)",
                },
            },
            "required": ["name", "location"],
        },
    },
    {
        "name": "delete_resource_group",
        "description": (
            "Delete an Azure resource group to clean up a prior SAO test "
            "deployment. Use only when the user explicitly asks to uninstall "
            "or clean up."
        ),
        "input_schema": {
            "type": "object",
            "properties": {
                "resource_group": {
                    "type": "string",
                    "description": "Resource group to delete",
                }
            },
            "required": ["resource_group"],
        },
    },
    {
        "name": "provision_infrastructure",
        "description": (
            "Deploy the SAO Bicep template to the active subscription and "
            "resource group. This is a provisioning action and must stay in "
            "its own phase."
        ),
        "input_schema": {
            "type": "object",
            "properties": {
                "resource_group": {
                    "type": "string",
                    "description": "Target resource group name",
                },
                "location": {
                    "type": "string",
                    "description": "Azure region",
                },
                "admin_oid": {
                    "type": "string",
                    "description": (
                        "Entra Object ID of the bootstrap admin"
                    ),
                },
            },
            "required": ["resource_group", "location", "admin_oid"],
        },
    },
    {
        "name": "check_deployment_status",
        "description": (
            "Read the deployment status and SAO health endpoint after "
            "provisioning completes."
        ),
        "input_schema": {
            "type": "object",
            "properties": {
                "resource_group": {
                    "type": "string",
                    "description": "Resource group where SAO is deployed",
                }
            },
            "required": ["resource_group"],
        },
    },
    {
        "name": "run_az_command",
        "description": (
            "Run an arbitrary Azure CLI command only when the specific tools "
            "above do not cover the operation. Provide args as an array of "
            "exact CLI tokens without the leading 'az'."
        ),
        "input_schema": {
            "type": "object",
            "properties": {
                "args": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": (
                        "Azure CLI argv tokens without the leading 'az'"
                    ),
                }
            },
            "required": ["args"],
        },
    },
]

CONTINUE_MESSAGE = "No questions right now. Please continue."
DEFAULT_DEPLOYMENT_NAME = "sao-bootstrap"
POLL_INTERVAL_SECONDS = 30
REQUIRED_CHECKIN = "Does this look correct? Do you have any questions before we continue?"
PHASE_DETAILS = {
    "authentication": {
        "title": "Authentication",
        "intro": (
            "I am about to start Azure device-code authentication so this "
            "session can prove your identity without creating any local "
            "credentials."
        ),
    },
    "read_only_discovery": {
        "title": "Read-only Discovery",
        "intro": (
            "I am about to run a read-only discovery batch so we can confirm "
            "who is signed in, inspect your subscriptions, and verify Azure "
            "permissions before we make any changes."
        ),
    },
    "subscription_selection": {
        "title": "Subscription Selection",
        "intro": (
            "I am about to set the active Azure subscription for the rest of "
            "this installer session so every later action lands in the right "
            "place."
        ),
    },
    "resource_group": {
        "title": "Resource Group",
        "intro": (
            "I am about to create the SAO resource group so the deployment has "
            "a dedicated boundary in Azure."
        ),
    },
    "cleanup": {
        "title": "Cleanup",
        "intro": (
            "I am about to remove the selected SAO test resource group. This "
            "is safe because Azure will delete only the resources contained in "
            "that dedicated group, not anything outside it."
        ),
    },
    "provisioning": {
        "title": "Provisioning",
        "intro": (
            "I am about to deploy the SAO infrastructure into Azure. This is "
            "the first major write phase and it will create the runtime "
            "resources the platform needs."
        ),
    },
    "verification": {
        "title": "Verification",
        "intro": (
            "I am about to run post-deployment verification checks so we can "
            "confirm the SAO endpoint is reachable and healthy."
        ),
    },
    "custom_command": {
        "title": "Custom Azure Command",
        "intro": (
            "I am about to run a custom Azure CLI command that falls outside "
            "the dedicated installer tools, so I want to make the exact action "
            "and its impact clear first."
        ),
    },
}


@dataclass(frozen=True)
class ToolExecutionPolicy:
    """Runtime controls for installer tool execution."""

    phase: str
    risk_class: str
    batchable: bool
    preview_text: str
    order: int


TOOL_POLICIES = {
    "az_login": ToolExecutionPolicy(
        phase="authentication",
        risk_class="write",
        batchable=False,
        preview_text="Azure device-code login",
        order=10,
    ),
    "get_signed_in_user": ToolExecutionPolicy(
        phase="read_only_discovery",
        risk_class="read",
        batchable=True,
        preview_text="signed-in user lookup",
        order=20,
    ),
    "list_subscriptions": ToolExecutionPolicy(
        phase="read_only_discovery",
        risk_class="read",
        batchable=True,
        preview_text="subscription inventory",
        order=30,
    ),
    "check_permissions": ToolExecutionPolicy(
        phase="read_only_discovery",
        risk_class="read",
        batchable=True,
        preview_text="Azure permission verification",
        order=40,
    ),
    "set_subscription": ToolExecutionPolicy(
        phase="subscription_selection",
        risk_class="write",
        batchable=False,
        preview_text="active subscription change",
        order=50,
    ),
    "create_resource_group": ToolExecutionPolicy(
        phase="resource_group",
        risk_class="write",
        batchable=False,
        preview_text="resource group creation",
        order=60,
    ),
    "delete_resource_group": ToolExecutionPolicy(
        phase="cleanup",
        risk_class="write",
        batchable=False,
        preview_text="resource group cleanup",
        order=65,
    ),
    "provision_infrastructure": ToolExecutionPolicy(
        phase="provisioning",
        risk_class="write",
        batchable=False,
        preview_text="SAO infrastructure deployment",
        order=70,
    ),
    "check_deployment_status": ToolExecutionPolicy(
        phase="verification",
        risk_class="read",
        batchable=False,
        preview_text="deployment health verification",
        order=80,
    ),
}


class InstallerAgent:
    """Claude-powered conversational agent for SAO installation."""

    def __init__(self, provider: str, api_key: str | None):
        self.client = (
            anthropic.Anthropic(api_key=api_key) if api_key else None
        )
        self.model = "claude-sonnet-4-20250514"
        self.conversation: list[dict] = []
        self.system_prompt = self._load_system_prompt()
        self.pending_phase_summary: str | None = None
        self.installer_state = {
            "admin_oid": None,
            "admin_upn": None,
            "subscription_id": None,
            "resource_group": None,
            "location": None,
            "deployment_name": None,
            "sao_endpoint": None,
        }

    def verify_connection(self) -> bool:
        """Quick API ping to verify the key works."""
        if self.client is None:
            return False
        try:
            resp = self.client.messages.create(
                model=self.model,
                max_tokens=50,
                messages=[{"role": "user", "content": "ping"}],
            )
            return resp.stop_reason is not None
        except Exception:
            return False

    def run(self):
        """Main conversation loop."""
        if self.client is None:
            raise RuntimeError("InstallerAgent.run() requires an API client.")
        self._append_user_message(
            (
                "I just connected. Please introduce yourself and "
                "start the installation process."
            )
        )

        while True:
            response = self.client.messages.create(
                model=self.model,
                max_tokens=4096,
                system=self.system_prompt,
                tools=TOOLS,
                messages=self.conversation,
            )

            assistant_content = response.content
            text_blocks = [block for block in assistant_content if block.type == "text"]
            tool_blocks = [
                block for block in assistant_content if block.type == "tool_use"
            ]

            if self.pending_phase_summary is not None and (
                tool_blocks or not self._summary_response_is_valid(text_blocks)
            ):
                self.conversation.append(
                    {"role": "assistant", "content": assistant_content}
                )
                self._append_user_message(
                    self._phase_summary_instruction(
                        self.pending_phase_summary
                    )
                )
                continue

            self.conversation.append(
                {"role": "assistant", "content": assistant_content}
            )

            for block in text_blocks:
                print(f"\n{block.text}")

            if tool_blocks:
                self.pending_phase_summary = self._execute_phase(tool_blocks)
                continue

            if response.stop_reason == "end_turn":
                self.pending_phase_summary = None
                user_input = input("\nYou: ")
                if user_input.lower() in ("exit", "quit", "q"):
                    print("Installation cancelled.")
                    break
                self._append_user_message(
                    user_input,
                    empty_fallback=CONTINUE_MESSAGE,
                )

    def run_cleanup_mode(self, resource_group: str) -> bool:
        """Run scripted cleanup mode without requiring the full LLM loop."""
        host_os = os.environ.get("HOST_OS", "windows")
        normalized_resource_group = resource_group.strip()
        if not normalized_resource_group:
            raise ValueError("Cleanup mode requires a resource group name.")

        print(f"\n{PHASE_DETAILS['cleanup']['title']}")
        print(
            "Here's what I'm about to do and why: "
            f"{PHASE_DETAILS['cleanup']['intro']}"
        )
        print("Planned Azure CLI commands:")
        for command in self._build_preview_commands(
            "delete_resource_group",
            {"resource_group": normalized_resource_group},
            host_os,
        ):
            print(f"  - {command}")
        print(f"Target resource group: {normalized_resource_group}")

        approved = self._confirm_yes_no(
            f"Approve cleanup of resource group {normalized_resource_group}? (y/n) "
        )
        if not approved:
            print("Cleanup cancelled. No Azure resources were changed.")
            return False

        print("\nRunning resource group cleanup...")
        result = self._dispatch_tool(
            "delete_resource_group",
            {"resource_group": normalized_resource_group},
        )
        if "COMMAND FAILED" in result or "COMMAND CANCELLED" in result:
            print(result)
            return False

        print(
            "\nCleanup summary: Azure accepted the deletion request for "
            f"{normalized_resource_group}. This is safe because the SAO test "
            "deployment lives inside that dedicated resource group, so Azure "
            "will remove only those contained resources together."
        )
        follow_up = input(f"\n{REQUIRED_CHECKIN}\nYou: ")
        normalized_follow_up = follow_up.strip()
        if normalized_follow_up:
            print(
                "\nCleanup follow-up: Once Azure finishes deleting the resource "
                "group, you can safely create a fresh SAO environment without "
                "any leftover test resources from this deployment."
            )

        rerun = self._confirm_yes_no(
            "Would you like fresh-install instructions now? (y/n) "
        )
        if rerun:
            print(
                "Re-run this bootstrapper without --cleanup to start a fresh "
                "install."
            )
        else:
            print(
                "You can re-run this bootstrapper without --cleanup whenever "
                "you want to start a fresh install."
            )
        return True

    def _append_user_message(
        self,
        content: str | list[dict[str, str]],
        empty_fallback: str | None = None,
    ) -> None:
        """Append a user message while guaranteeing non-empty content."""
        normalized = self._normalize_user_content(
            content, empty_fallback=empty_fallback
        )
        self.conversation.append({"role": "user", "content": normalized})

    def _normalize_user_content(
        self,
        content: str | list[dict[str, str]],
        empty_fallback: str | None = None,
    ) -> str | list[dict[str, str]]:
        """Prevent empty user content from reaching the Anthropic API."""
        if isinstance(content, str):
            normalized_text = content.strip()
            if normalized_text:
                return normalized_text
            if empty_fallback is not None:
                return empty_fallback
            raise ValueError("User message content cannot be empty.")

        if content:
            return content
        if empty_fallback is not None:
            return empty_fallback
        raise ValueError("User message content cannot be empty.")

    def _summary_response_is_valid(self, text_blocks: list[Any]) -> bool:
        """Require a plain-English summary plus the exact phase check-in."""
        combined_text = "\n".join(
            block.text.strip() for block in text_blocks if block.text.strip()
        )
        return REQUIRED_CHECKIN in combined_text

    def _phase_summary_instruction(self, phase: str) -> str:
        """Tell the model to summarize the phase before continuing."""
        phase_title = PHASE_DETAILS[phase]["title"]
        return (
            f"The {phase_title} phase is complete. Respond with plain text only. "
            "Give a brief 1-2 sentence summary of what happened and what it "
            f"means. Then ask exactly: {REQUIRED_CHECKIN} "
            "Do not call any tools in this response."
        )

    def _get_tool_policy(
        self, name: str, args: dict[str, Any]
    ) -> ToolExecutionPolicy:
        """Return execution policy metadata for a tool call."""
        if name == "run_az_command":
            from tools.azure import is_safe_read_only_az_args

            command_args = args.get("args", [])
            if is_safe_read_only_az_args(command_args):
                return ToolExecutionPolicy(
                    phase="read_only_discovery",
                    risk_class="read",
                    batchable=True,
                    preview_text="custom read-only Azure inspection",
                    order=45,
                )
            return ToolExecutionPolicy(
                phase="custom_command",
                risk_class="write",
                batchable=False,
                preview_text="custom Azure CLI action",
                order=90,
            )

        policy = TOOL_POLICIES.get(name)
        if policy is None:
            return ToolExecutionPolicy(
                phase="custom_command",
                risk_class="write",
                batchable=False,
                preview_text=name,
                order=100,
            )
        return policy

    def _build_preview_commands(
        self, name: str, args: dict[str, Any], host_os: str
    ) -> list[str]:
        """Render human-readable command previews for a tool call."""
        from tools.azure import format_az_command
        from tools.validator import describe_permission_check_commands

        if name == "az_login":
            return [
                format_az_command(
                    ["login", "--use-device-code"], host_os=host_os
                )
            ]

        if name == "get_signed_in_user":
            return [
                format_az_command(
                    [
                        "ad",
                        "signed-in-user",
                        "show",
                        "--query",
                        "{oid:id, upn:userPrincipalName, name:displayName}",
                        "--output",
                        "json",
                    ],
                    host_os=host_os,
                )
            ]

        if name == "list_subscriptions":
            return [
                format_az_command(
                    [
                        "account",
                        "list",
                        "--query",
                        "[].{id:id, name:name, state:state}",
                        "--output",
                        "json",
                    ],
                    host_os=host_os,
                )
            ]

        if name == "check_permissions":
            return describe_permission_check_commands(
                admin_oid=self.installer_state["admin_oid"],
                subscription_id=self.installer_state["subscription_id"],
                host_os=host_os,
            )

        if name == "set_subscription":
            return [
                format_az_command(
                    [
                        "account",
                        "set",
                        "--subscription",
                        args["subscription_id"],
                    ],
                    host_os=host_os,
                )
            ]

        if name == "create_resource_group":
            return [
                format_az_command(
                    [
                        "group",
                        "create",
                        "--name",
                        args["name"],
                        "--location",
                        args["location"],
                    ],
                    host_os=host_os,
                )
            ]

        if name == "delete_resource_group":
            return [
                format_az_command(
                    [
                        "group",
                        "delete",
                        "--name",
                        args["resource_group"],
                        "--yes",
                    ],
                    host_os=host_os,
                )
            ]

        if name == "provision_infrastructure":
            return [
                format_az_command(
                    [
                        "deployment",
                        "group",
                        "create",
                        "--name",
                        DEFAULT_DEPLOYMENT_NAME,
                        "--resource-group",
                        args["resource_group"],
                        "--template-file",
                        "/app/bicep/main.bicep",
                        "--parameters",
                        f"location={args['location']}",
                        f"adminOid={args['admin_oid']}",
                        "saoImageTag=latest",
                        "--no-wait",
                        "--output",
                        "json",
                    ],
                    host_os=host_os,
                ),
                format_az_command(
                    [
                        "deployment",
                        "group",
                        "show",
                        "--resource-group",
                        args["resource_group"],
                        "--name",
                        DEFAULT_DEPLOYMENT_NAME,
                        "--query",
                        "{state:properties.provisioningState, timestamp:properties.timestamp}",
                        "--output",
                        "json",
                    ],
                    host_os=host_os,
                ),
            ]

        if name == "check_deployment_status":
            return [
                format_az_command(
                    [
                        "containerapp",
                        "show",
                        "--name",
                        "sao-app",
                        "--resource-group",
                        args["resource_group"],
                        "--query",
                        "properties.configuration.ingress.fqdn",
                        "-o",
                        "tsv",
                    ],
                    host_os=host_os,
                ),
                format_az_command(
                    [
                        "rest",
                        "--method",
                        "GET",
                        "--url",
                        "https://<resolved-fqdn>/api/health",
                    ],
                    host_os=host_os,
                ),
            ]

        if name == "run_az_command":
            return [format_az_command(args["args"], host_os=host_os)]

        return []

    def _confirm_yes_no(self, prompt: str) -> bool:
        """Prompt until the user explicitly approves or declines."""
        while True:
            try:
                answer = input(prompt).strip().lower()
            except EOFError:
                return False
            if answer in {"y", "n"}:
                return answer == "y"
            print("Please enter 'y' or 'n'.")

    def _select_phase_blocks(
        self, tool_blocks: list[Any]
    ) -> tuple[str, list[Any], list[Any]]:
        """Execute only the first major phase from a response."""
        first_policy = self._get_tool_policy(
            tool_blocks[0].name, dict(tool_blocks[0].input)
        )
        selected: list[Any] = []
        deferred: list[Any] = []
        for block in tool_blocks:
            policy = self._get_tool_policy(block.name, dict(block.input))
            if policy.phase == first_policy.phase:
                selected.append(block)
            else:
                deferred.append(block)
        selected.sort(
            key=lambda block: self._get_tool_policy(
                block.name, dict(block.input)
            ).order
        )
        return first_policy.phase, selected, deferred

    def _try_parse_json(self, result: str) -> dict[str, Any] | list[Any] | None:
        """Parse JSON tool output when available."""
        try:
            return json.loads(result)
        except (json.JSONDecodeError, TypeError):
            return None

    def _format_elapsed(self, elapsed_seconds: float) -> str:
        """Render elapsed seconds as human-friendly text."""
        rounded = max(0, int(elapsed_seconds))
        minutes, seconds = divmod(rounded, 60)
        if minutes and seconds:
            minute_label = "minute" if minutes == 1 else "minutes"
            second_label = "second" if seconds == 1 else "seconds"
            return f"{minutes} {minute_label} {seconds} {second_label}"
        if minutes:
            minute_label = "minute" if minutes == 1 else "minutes"
            return f"{minutes} {minute_label}"
        second_label = "second" if rounded == 1 else "seconds"
        return f"{rounded} {second_label}"

    def _infer_provisioning_stage(
        self, resource_group: str, host_os: str
    ) -> str:
        """Infer a likely provisioning stage from visible resource types."""
        from tools.azure import list_resource_group_resource_types

        resource_types_result = list_resource_group_resource_types(
            resource_group, host_os=host_os
        )
        if "COMMAND FAILED" in resource_types_result:
            return "Azure is still applying the SAO infrastructure changes."

        parsed_types = self._try_parse_json(resource_types_result)
        if not isinstance(parsed_types, list):
            return "Azure is still applying the SAO infrastructure changes."

        resource_types = {str(item) for item in parsed_types}
        if "Microsoft.DBforPostgreSQL/flexibleServers" not in resource_types:
            return "Provisioning PostgreSQL."
        if "Microsoft.KeyVault/vaults" not in resource_types:
            return "Provisioning Key Vault."
        if (
            "Microsoft.OperationalInsights/workspaces" not in resource_types
            or "Microsoft.App/managedEnvironments" not in resource_types
        ):
            return "Provisioning the Container Apps environment."
        if "Microsoft.App/containerApps" not in resource_types:
            return "Provisioning the SAO application container."
        return "Finalizing the SAO application endpoint."

    def _answer_polling_question(
        self, question: str, status_snapshot: dict[str, Any]
    ) -> None:
        """Answer a user question during deployment polling without tools."""
        response = self.client.messages.create(
            model=self.model,
            max_tokens=1024,
            system=(
                self.system_prompt
                + "\n\nYou are temporarily answering a question during an active "
                "Azure deployment poll. Answer the question in plain English only. "
                "Do not call tools. Do not advance the installer to a new phase."
            ),
            messages=[
                {
                    "role": "user",
                    "content": (
                        "The operator asked this question during deployment "
                        f"polling: {question}\n\nCurrent deployment snapshot:\n"
                        f"{json.dumps(status_snapshot, indent=2)}"
                    ),
                }
            ],
        )

        for block in response.content:
            if block.type == "text" and block.text.strip():
                print(f"\n{block.text}")

    def _print_provisioning_handoff(
        self,
        endpoint: str,
        admin_oid: str,
        health_result: str,
    ) -> None:
        """Print the final deployment handoff for the operator."""
        display_endpoint = endpoint
        if display_endpoint and not display_endpoint.startswith("http"):
            display_endpoint = f"https://{display_endpoint}"

        health_summary = health_result
        if health_result.startswith("Endpoint: "):
            for line in health_result.splitlines():
                if line.startswith("Health: "):
                    health_summary = line[len("Health: ") :]
                    break

        print("\nProvisioning complete.")
        print(f"SAO endpoint: {display_endpoint}")
        print(f"Bootstrap admin OID: {admin_oid}")
        print("Next steps:")
        print("  1. Open the SAO endpoint in your browser.")
        print("  2. Sign in with your Entra ID account.")
        print("  3. Let the SAO agent guide the remaining role configuration.")
        print(
            "No passwords were created. Access is controlled entirely through "
            "your organization's Entra ID."
        )
        if health_summary:
            print(f"Health check: {health_summary}")

    def _run_provisioning_with_polling(
        self, args: dict[str, Any], host_os: str
    ) -> str:
        """Start the Azure deployment and poll until it reaches a terminal state."""
        from tools.azure import (
            check_deployment_status,
            get_group_deployment_endpoint,
            get_group_deployment_status,
            start_infrastructure_provisioning,
        )

        resource_group = args["resource_group"]
        location = args["location"]
        admin_oid = args["admin_oid"]
        deployment_name = DEFAULT_DEPLOYMENT_NAME

        self.installer_state["deployment_name"] = deployment_name
        self.installer_state["resource_group"] = resource_group
        self.installer_state["location"] = location
        self.installer_state["admin_oid"] = admin_oid

        start_result = start_infrastructure_provisioning(
            resource_group=resource_group,
            location=location,
            admin_oid=admin_oid,
            host_os=host_os,
            deployment_name=deployment_name,
        )
        if "COMMAND FAILED" in start_result or "COMMAND CANCELLED" in start_result:
            return start_result

        poll_started_at = time.monotonic()

        while True:
            status_result = get_group_deployment_status(
                resource_group=resource_group,
                deployment_name=deployment_name,
                host_os=host_os,
            )
            if "COMMAND FAILED" in status_result:
                return status_result

            parsed_status = self._try_parse_json(status_result)
            if not isinstance(parsed_status, dict):
                return (
                    "COMMAND FAILED: Unable to parse deployment status for "
                    f"{deployment_name}."
                )

            state = str(parsed_status.get("state", "Unknown"))
            elapsed = self._format_elapsed(time.monotonic() - poll_started_at)

            if state == "Succeeded":
                endpoint_result = get_group_deployment_endpoint(
                    resource_group=resource_group,
                    deployment_name=deployment_name,
                    host_os=host_os,
                )
                endpoint = endpoint_result.strip()
                if (
                    not endpoint
                    or "COMMAND FAILED" in endpoint_result
                    or "COMMAND CANCELLED" in endpoint_result
                ):
                    endpoint = "<endpoint unavailable>"
                self.installer_state["sao_endpoint"] = (
                    endpoint
                    if endpoint.startswith("http")
                    else f"https://{endpoint}"
                    if endpoint != "<endpoint unavailable>"
                    else endpoint
                )

                health_result = check_deployment_status(
                    resource_group, host_os=host_os
                )
                if (
                    "COMMAND FAILED" not in health_result
                    and "COMMAND CANCELLED" not in health_result
                ):
                    self._update_state(
                        "check_deployment_status",
                        {"resource_group": resource_group},
                        health_result,
                    )
                    if self.installer_state["sao_endpoint"]:
                        endpoint = self.installer_state["sao_endpoint"]

                self._print_provisioning_handoff(
                    endpoint=endpoint,
                    admin_oid=admin_oid,
                    health_result=health_result,
                )

                result = {
                    "deployment_name": deployment_name,
                    "provisioning_state": state,
                    "elapsed": elapsed,
                    "resource_group": resource_group,
                    "endpoint": endpoint,
                    "admin_oid": admin_oid,
                    "health": health_result,
                }
                return json.dumps(result, indent=2)

            if state in {"Failed", "Canceled"}:
                timestamp = parsed_status.get("timestamp", "unknown time")
                return (
                    "COMMAND FAILED: Azure deployment "
                    f"{deployment_name} ended with state {state} at {timestamp}."
                )

            stage_message = self._infer_provisioning_stage(resource_group, host_os)
            print(
                "\nDeployment is still running "
                f"({elapsed} elapsed). {stage_message}"
            )

            user_input = input(
                "\nDuring provisioning, press Enter to keep waiting, "
                "type 'status' to refresh now, or ask a question: "
            )
            normalized_input = user_input.strip()
            if not normalized_input:
                time.sleep(POLL_INTERVAL_SECONDS)
                continue
            if normalized_input.lower() == "status":
                continue

            self._answer_polling_question(
                normalized_input,
                {
                    "deployment_name": deployment_name,
                    "resource_group": resource_group,
                    "status": parsed_status,
                    "elapsed": elapsed,
                    "admin_oid": admin_oid,
                },
            )
            time.sleep(POLL_INTERVAL_SECONDS)

    def _execute_phase(self, tool_blocks: list[Any]) -> str:
        """Explain, approve, execute, and summarize a single phase."""
        host_os = os.environ.get("HOST_OS", "windows")
        os.environ["HOST_OS"] = host_os
        phase, selected_blocks, deferred_blocks = self._select_phase_blocks(
            tool_blocks
        )
        phase_detail = PHASE_DETAILS[phase]
        selected_policies = [
            self._get_tool_policy(block.name, dict(block.input))
            for block in selected_blocks
        ]
        batch_read_only = all(
            policy.risk_class == "read" and policy.batchable
            for policy in selected_policies
        )

        preview_commands: list[str] = []
        for block in selected_blocks:
            preview_commands.extend(
                self._build_preview_commands(
                    block.name, dict(block.input), host_os
                )
            )

        print(f"\n{phase_detail['title']}")
        print(f"Here's what I'm about to do and why: {phase_detail['intro']}")
        if preview_commands:
            print("Planned Azure CLI commands:")
            for command in preview_commands:
                print(f"  - {command}")
        if batch_read_only:
            print(
                "This is a read-only batch. One approval will cover every "
                "discovery and permission check listed above."
            )
        else:
            print(
                "This phase changes Azure state or moves the install forward, "
                "so I will ask for explicit confirmation before running it."
            )

        tool_results: list[dict[str, str]] = []

        if batch_read_only:
            approved = self._confirm_yes_no(
                "Approve this read-only batch? (y/n) "
            )
            if approved:
                for block in selected_blocks:
                    policy = self._get_tool_policy(
                        block.name, dict(block.input)
                    )
                    print(f"\nRunning {policy.preview_text}...")
                    tool_results.append(
                        {
                            "type": "tool_result",
                            "tool_use_id": block.id,
                            "content": self._dispatch_tool(
                                block.name, dict(block.input)
                            ),
                        }
                    )
            else:
                for block in selected_blocks:
                    tool_results.append(
                        {
                            "type": "tool_result",
                            "tool_use_id": block.id,
                            "content": (
                                "COMMAND CANCELLED: User declined the read-only "
                                "batch."
                            ),
                        }
                    )
        else:
            for block in selected_blocks:
                policy = self._get_tool_policy(block.name, dict(block.input))
                approved = self._confirm_yes_no(
                    f"Approve {policy.preview_text}? (y/n) "
                )
                if approved:
                    print(f"\nRunning {policy.preview_text}...")
                    if block.name == "provision_infrastructure":
                        result = self._run_provisioning_with_polling(
                            dict(block.input), host_os
                        )
                    else:
                        result = self._dispatch_tool(
                            block.name, dict(block.input)
                        )
                else:
                    result = "COMMAND CANCELLED: User declined this step."
                tool_results.append(
                    {
                        "type": "tool_result",
                        "tool_use_id": block.id,
                        "content": result,
                    }
                )

        if deferred_blocks:
            print(
                "\nI am stopping after this phase so we can review the result "
                "before any later phase runs."
            )
            for block in deferred_blocks:
                tool_results.append(
                    {
                        "type": "tool_result",
                        "tool_use_id": block.id,
                        "content": (
                            "DEFERRED: The installer executes one major phase "
                            "per turn. Ask again after the current phase review."
                        ),
                    }
                )

        self._append_user_message(
            [
                *tool_results,
                {
                    "type": "text",
                    "text": self._phase_summary_instruction(phase),
                },
            ]
        )
        return phase

    def _dispatch_tool(self, name: str, args: dict) -> str:
        """Route tool calls to implementations."""
        host_os = os.environ.get("HOST_OS", "windows")
        os.environ["HOST_OS"] = host_os

        from tools.azure import (
            az_login,
            check_deployment_status,
            create_resource_group,
            delete_resource_group,
            get_signed_in_user,
            list_subscriptions,
            provision_infrastructure,
            run_az_command,
            set_subscription,
        )
        from tools.validator import check_permissions

        dispatch = {
            "az_login": lambda: az_login(host_os=host_os),
            "get_signed_in_user": lambda: get_signed_in_user(host_os=host_os),
            "list_subscriptions": lambda: list_subscriptions(
                host_os=host_os
            ),
            "set_subscription": lambda: set_subscription(
                args["subscription_id"], host_os=host_os
            ),
            "check_permissions": lambda: check_permissions(
                admin_oid=self.installer_state["admin_oid"],
                subscription_id=self.installer_state["subscription_id"],
                host_os=host_os,
            ),
            "create_resource_group": lambda: create_resource_group(
                args["name"], args["location"], host_os=host_os
            ),
            "delete_resource_group": lambda: delete_resource_group(
                args["resource_group"], host_os=host_os
            ),
            "provision_infrastructure": lambda: provision_infrastructure(
                args["resource_group"],
                args["location"],
                args["admin_oid"],
                host_os=host_os,
            ),
            "check_deployment_status": lambda: check_deployment_status(
                args["resource_group"], host_os=host_os
            ),
            "run_az_command": lambda: run_az_command(
                args["args"], host_os=host_os
            ),
        }

        fn = dispatch.get(name)
        if fn is None:
            return f"Unknown tool: {name}"

        try:
            result = fn()
            self._update_state(name, args, result)
            return result
        except Exception as exc:
            return f"Error executing {name}: {str(exc)}"

    def _update_state(self, tool_name: str, args: dict, result: str):
        """Track installer state from tool outputs."""
        if "COMMAND FAILED" in result or "COMMAND CANCELLED" in result:
            return

        try:
            parsed = json.loads(result)
        except (json.JSONDecodeError, TypeError):
            parsed = None

        if tool_name == "get_signed_in_user" and parsed:
            self.installer_state["admin_oid"] = parsed.get("oid")
            self.installer_state["admin_upn"] = parsed.get("upn")

        elif tool_name == "set_subscription":
            self.installer_state["subscription_id"] = args.get(
                "subscription_id"
            )

        elif tool_name == "create_resource_group":
            self.installer_state["resource_group"] = args.get("name")
            self.installer_state["location"] = args.get("location")

        elif tool_name == "delete_resource_group":
            self.installer_state["resource_group"] = None
            self.installer_state["location"] = None
            self.installer_state["deployment_name"] = None
            self.installer_state["sao_endpoint"] = None

        elif tool_name == "check_deployment_status":
            for line in result.splitlines():
                if line.startswith("Endpoint: "):
                    self.installer_state["sao_endpoint"] = line[
                        len("Endpoint: ") :
                    ]
                    break

    def _load_system_prompt(self) -> str:
        """Load the system prompt from the bundled markdown file."""
        for path in [
            Path("/app/system_prompt.md"),
            Path(__file__).parent.parent / "system_prompt.md",
        ]:
            if path.exists():
                return path.read_text()
        raise FileNotFoundError(
            "system_prompt.md not found. "
            "Expected at /app/system_prompt.md or installer/system_prompt.md"
        )
