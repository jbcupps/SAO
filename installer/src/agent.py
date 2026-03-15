"""SAO Installer — Conversation manager and tool dispatch."""

import json
import os
import re
import time
from pathlib import Path
from typing import Any
from uuid import uuid4

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
                "image_reference": {
                    "type": "string",
                    "description": (
                        "Optional full container image reference override for "
                        "the SAO application"
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
        "name": "review_last_failure",
        "description": (
            "Review the structured diagnostics bundle for the most recent "
            "bootstrap failure. Use this before any troubleshooting or "
            "recovery action."
        ),
        "input_schema": {
            "type": "object",
            "properties": {},
            "required": [],
        },
    },
    {
        "name": "apply_guided_fix",
        "description": (
            "Apply a supported recovery action for the most recent bootstrap "
            "failure. Supported actions are purge_deleted_key_vault, "
            "retry_with_name_suffix, retry_with_image_override, and "
            "cleanup_resource_group."
        ),
        "input_schema": {
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "description": "Typed recovery action to apply",
                    "enum": [
                        "purge_deleted_key_vault",
                        "retry_with_name_suffix",
                        "retry_with_image_override",
                        "cleanup_resource_group",
                    ],
                },
                "name_suffix": {
                    "type": "string",
                    "description": (
                        "Optional short suffix override for retry_with_name_suffix"
                    ),
                },
                "image_reference": {
                    "type": "string",
                    "description": (
                        "Full container image reference to use for "
                        "retry_with_image_override"
                    ),
                },
                "resource_group": {
                    "type": "string",
                    "description": (
                        "Optional resource group override for cleanup_resource_group"
                    ),
                },
            },
            "required": ["action"],
        },
    },
    {
        "name": "run_az_command",
        "description": (
            "Run an arbitrary Azure CLI command only when the operator "
            "explicitly asks for an unsupported Azure action. Do not use this "
            "for normal deployment diagnostics that are already covered by "
            "review_last_failure. Provide args as an array of exact CLI tokens "
            "without the leading 'az'."
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
REVIEW_CHECKINS = (
    REQUIRED_CHECKIN,
    "Does that line up with what you expected? Any questions before we continue?",
    "Does that all look right? Any questions before we continue?",
    "Does that match what you wanted to see? Any questions before we continue?",
)
PHASE_CHECKINS = {
    "authentication": REVIEW_CHECKINS[0],
    "read_only_discovery": REVIEW_CHECKINS[1],
    "subscription_selection": REVIEW_CHECKINS[3],
    "resource_group": REVIEW_CHECKINS[2],
    "cleanup": REVIEW_CHECKINS[2],
    "provisioning": REVIEW_CHECKINS[1],
    "troubleshooting_review": REVIEW_CHECKINS[3],
    "troubleshooting_fix": REVIEW_CHECKINS[2],
    "verification": REVIEW_CHECKINS[0],
    "custom_command": REVIEW_CHECKINS[1],
}
PHASE_DETAILS = {
    "authentication": {
        "title": "Authentication",
        "lead_in": "Here's what I'm about to do and why:",
        "intro": (
            "start Azure device-code authentication so this session can prove "
            "your identity without creating any local credentials."
        ),
    },
    "read_only_discovery": {
        "title": "Read-only Discovery",
        "lead_in": "Next I need to",
        "intro": (
            "run a read-only discovery batch so we can confirm who is signed "
            "in, inspect your subscriptions, and verify Azure permissions "
            "before we make any changes."
        ),
    },
    "subscription_selection": {
        "title": "Subscription Selection",
        "lead_in": "Now we're ready to",
        "intro": (
            "set the active Azure subscription for the rest of this installer "
            "session so every later action lands in the right place."
        ),
    },
    "resource_group": {
        "title": "Resource Group",
        "lead_in": "Next I need to",
        "intro": (
            "create the SAO resource group so the deployment has a dedicated "
            "boundary in Azure."
        ),
    },
    "cleanup": {
        "title": "Cleanup",
        "lead_in": "Let me quickly confirm the cleanup plan:",
        "intro": (
            "remove the selected SAO test resource group. This is safe because "
            "Azure will delete only the resources contained in that dedicated "
            "group, not anything outside it."
        ),
    },
    "provisioning": {
        "title": "Provisioning",
        "lead_in": "Now we're ready to",
        "intro": (
            "deploy the SAO infrastructure into Azure. This is the main write "
            "phase and it will create the runtime resources the platform needs."
        ),
    },
    "troubleshooting_review": {
        "title": "Troubleshooting Review",
        "lead_in": "Let me quickly check",
        "intro": (
            "the structured deployment diagnostics we already collected so I "
            "can explain the failure clearly instead of guessing at Azure CLI "
            "syntax."
        ),
    },
    "troubleshooting_fix": {
        "title": "Troubleshooting Fix",
        "lead_in": "Next I need to",
        "intro": (
            "apply the recovery path you chose and, when it makes sense, carry "
            "that straight into a clean retry."
        ),
    },
    "verification": {
        "title": "Verification",
        "lead_in": "Let me quickly check",
        "intro": (
            "the deployed endpoint and health status so we can confirm the SAO "
            "environment is reachable and healthy."
        ),
    },
    "custom_command": {
        "title": "Custom Azure Command",
        "lead_in": "Let me confirm this custom Azure command:",
        "intro": (
            "I want to make the exact action and its impact clear because it "
            "falls outside the dedicated installer tools."
        ),
    },
}

class ToolExecutionPolicy:
    """Runtime controls for installer tool execution."""

    def __init__(
        self,
        phase: str,
        risk_class: str,
        batchable: bool,
        preview_text: str,
        order: int,
    ):
        self.phase = phase
        self.risk_class = risk_class
        self.batchable = batchable
        self.preview_text = preview_text
        self.order = order


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
    "review_last_failure": ToolExecutionPolicy(
        phase="troubleshooting_review",
        risk_class="read",
        batchable=False,
        preview_text="structured deployment failure review",
        order=75,
    ),
    "apply_guided_fix": ToolExecutionPolicy(
        phase="troubleshooting_fix",
        risk_class="write",
        batchable=False,
        preview_text="guided recovery action",
        order=77,
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
        self.base_system_prompt = self._load_system_prompt()
        self.pending_phase_summary: str | None = None
        self.installer_state = {
            "admin_oid": None,
            "admin_upn": None,
            "subscription_id": None,
            "resource_group": None,
            "location": None,
            "deployment_name": None,
            "name_suffix": "",
            "image_override": None,
            "sao_endpoint": None,
            "last_failure_bundle": None,
            "troubleshooting_active": False,
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
                system=self._build_runtime_system_prompt(),
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
        print(self._phase_intro_text("cleanup"))
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

        print("\nStarting resource group cleanup...")
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
        follow_up = input(f"\n{self._checkin_for_phase('cleanup')}\nYou: ")
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
        """Require a plain-English summary plus an approved phase check-in."""
        combined_text = "\n".join(
            block.text.strip() for block in text_blocks if block.text.strip()
        )
        return any(checkin in combined_text for checkin in REVIEW_CHECKINS)

    def _checkin_for_phase(self, phase: str) -> str:
        """Return the approved review prompt for the current phase."""
        return PHASE_CHECKINS.get(phase, REQUIRED_CHECKIN)

    def _phase_summary_instruction(self, phase: str) -> str:
        """Tell the model to summarize the phase before continuing."""
        phase_title = PHASE_DETAILS[phase]["title"]
        review_prompt = self._checkin_for_phase(phase)
        return (
            f"The {phase_title} phase is complete. Respond with plain text only. "
            "Give a brief 1-2 sentence summary of what happened and what it "
            f"means. Then ask exactly: {review_prompt} "
            "Do not call any tools in this response."
        )

    def _phase_intro_text(self, phase: str) -> str:
        """Render a natural-sounding phase introduction."""
        phase_detail = PHASE_DETAILS[phase]
        lead_in = phase_detail.get("lead_in", "Here's what I'm about to do and why:")
        intro = phase_detail["intro"]
        if lead_in.endswith(":"):
            return f"{lead_in} {intro}"
        return f"{lead_in} {intro}"

    def _build_runtime_system_prompt(self) -> str:
        """Append live troubleshooting context to the static system prompt."""
        prompt = self.base_system_prompt
        if not self.installer_state.get("troubleshooting_active"):
            return prompt

        failure_bundle = self.installer_state.get("last_failure_bundle")
        if not isinstance(failure_bundle, dict) or not failure_bundle:
            return (
                prompt
                + "\n\nTroubleshooting mode is active. Use review_last_failure "
                "before any custom Azure diagnostics and use apply_guided_fix "
                "for supported recovery actions."
            )

        runtime_bundle = {
            "issue_type": failure_bundle.get("issue_type"),
            "diagnosis": failure_bundle.get("diagnosis"),
            "failed_resource": failure_bundle.get("failed_resource"),
            "resource_group": failure_bundle.get("resource_group"),
            "deployment_name": failure_bundle.get("deployment_name"),
            "image_reference": failure_bundle.get("image_reference"),
            "suggested_actions": failure_bundle.get("suggested_actions", [])[:4],
            "evidence": failure_bundle.get("evidence", [])[:4],
        }
        return (
            prompt
            + "\n\nTroubleshooting mode is active for the most recent bootstrap "
            "failure. Use review_last_failure before any new deployment "
            "diagnostic query, prefer apply_guided_fix for supported recovery, "
            "and only use run_az_command when the operator explicitly asks for "
            "an unsupported Azure CLI action.\n\nLast failure snapshot:\n"
            + json.dumps(runtime_bundle, indent=2)
        )

    def _normalize_name_suffix(self, value: str | None) -> str:
        """Clamp suggested name suffixes to the short Azure-safe form we use."""
        normalized = re.sub(r"[^a-z0-9]", "", (value or "").lower())
        return normalized[:3]

    def _suggest_name_suffix(self) -> str:
        """Generate a short suffix for collision retries."""
        suggested = self._normalize_name_suffix(uuid4().hex)
        return suggested or "001"

    def _dedupe_lines(self, lines: list[str]) -> list[str]:
        """Preserve order while removing duplicate diagnostic lines."""
        seen: set[str] = set()
        unique_lines: list[str] = []
        for line in lines:
            normalized = line.strip()
            if not normalized or normalized in seen:
                continue
            seen.add(normalized)
            unique_lines.append(normalized)
        return unique_lines

    def _extract_error_messages(self, payload: Any) -> list[str]:
        """Flatten nested Azure error payloads into readable lines."""
        if payload is None:
            return []

        if isinstance(payload, str):
            text = payload.strip()
            if not text or text.lower() == "null":
                return []
            parsed = self._try_parse_json(text)
            if parsed is not None and parsed != text:
                return self._extract_error_messages(parsed)
            if "\n" in text:
                return self._dedupe_lines(
                    [line.strip() for line in text.splitlines() if line.strip()]
                )
            return [text]

        if isinstance(payload, list):
            messages: list[str] = []
            for item in payload:
                messages.extend(self._extract_error_messages(item))
            return self._dedupe_lines(messages)

        if isinstance(payload, dict):
            messages: list[str] = []
            code = payload.get("code")
            message = payload.get("message")
            if code and message:
                messages.append(f"{code}: {message}")
            elif message:
                messages.append(str(message))
            elif code and not any(
                key in payload
                for key in ("error", "details", "innererror", "statusMessage")
            ):
                messages.append(str(code))

            for key in (
                "error",
                "details",
                "innererror",
                "statusMessage",
                "additionalInfo",
                "info",
            ):
                if key in payload:
                    messages.extend(self._extract_error_messages(payload[key]))
            return self._dedupe_lines(messages)

        return [str(payload)]

    def _extract_failed_operation_summaries(
        self, operations_payload: Any
    ) -> list[str]:
        """Summarize failed ARM deployment operations."""
        if not isinstance(operations_payload, list):
            return []

        summaries: list[str] = []
        for operation in operations_payload:
            if not isinstance(operation, dict):
                continue
            properties = operation.get("properties", {})
            if not isinstance(properties, dict):
                continue

            state = str(
                properties.get(
                    "provisioningState",
                    operation.get("provisioningState", ""),
                )
            )
            status_messages = self._extract_error_messages(
                properties.get("statusMessage")
            )
            if state not in {"Failed", "Canceled"} and not status_messages:
                continue

            target_resource = properties.get("targetResource", {})
            if not isinstance(target_resource, dict):
                target_resource = {}

            resource_type = str(
                target_resource.get("resourceType")
                or target_resource.get("type")
                or ""
            ).strip()
            resource_name = str(
                target_resource.get("resourceName")
                or target_resource.get("name")
                or ""
            ).strip()
            target_label = "/".join(
                part for part in (resource_type, resource_name) if part
            )
            if not target_label:
                target_label = "Deployment operation"

            summary = target_label
            if state:
                summary = f"{summary} ({state})"
            if status_messages:
                summary = f"{summary}: {status_messages[0]}"
            summaries.append(summary)

        return self._dedupe_lines(summaries)

    def _extract_key_vault_name(self, text: str) -> str | None:
        """Pull the most likely Key Vault name out of Azure error text."""
        lowered_text = text.lower()
        patterns = [
            r"(?:key vault|vault)\s+'([a-z0-9][a-z0-9-]{1,22})'",
            r'"([a-z0-9][a-z0-9-]{1,22}-kv)"',
            r"\b([a-z0-9][a-z0-9-]{1,22}-kv)\b",
        ]
        for pattern in patterns:
            match = re.search(pattern, lowered_text)
            if match:
                return match.group(1)
        return None

    def _find_deleted_key_vault(
        self, deleted_vaults_payload: Any, vault_name: str
    ) -> dict[str, str] | None:
        """Match a deleted Key Vault payload entry by name."""
        if not isinstance(deleted_vaults_payload, list):
            return None

        normalized_name = vault_name.strip().lower()
        for entry in deleted_vaults_payload:
            if not isinstance(entry, dict):
                continue

            candidate_names = [
                entry.get("name"),
                entry.get("vaultName"),
            ]
            properties = entry.get("properties", {})
            if not isinstance(properties, dict):
                properties = {}
            else:
                candidate_names.append(properties.get("vaultName"))

            if normalized_name not in {
                str(name).strip().lower()
                for name in candidate_names
                if name
            }:
                continue

            location = str(
                entry.get("location")
                or properties.get("location")
                or properties.get("scheduledPurgeDate")
                or ""
            ).strip()
            if location and "t" in location.lower() and ":" in location:
                location = ""

            return {
                "name": normalized_name,
                "location": location,
            }
        return None

    def _get_tool_policy(
        self, name: str, args: dict[str, Any]
    ) -> ToolExecutionPolicy:
        """Return execution policy metadata for a tool call."""
        if name == "apply_guided_fix":
            action = str(args.get("action") or "").strip()
            preview_text = "guided recovery action"
            if action == "purge_deleted_key_vault":
                preview_text = "guided Key Vault purge and retry"
            elif action == "retry_with_name_suffix":
                preview_text = "guided retry with a unique suffix"
            elif action == "retry_with_image_override":
                preview_text = "guided retry with an alternate image"
            elif action == "cleanup_resource_group":
                preview_text = "guided resource group cleanup"
            return ToolExecutionPolicy(
                phase="troubleshooting_fix",
                risk_class="write",
                batchable=False,
                preview_text=preview_text,
                order=77,
            )

        if name == "run_az_command":
            from tools.azure import is_safe_read_only_az_args

            command_args = args.get("args", [])
            if is_safe_read_only_az_args(command_args):
                phase = (
                    "troubleshooting_review"
                    if self.installer_state.get("troubleshooting_active")
                    else "read_only_discovery"
                )
                return ToolExecutionPolicy(
                    phase=phase,
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
            name_suffix = self._normalize_name_suffix(
                args.get("name_suffix")
                or self.installer_state.get("name_suffix")
            )
            image_reference = str(
                args.get("image_reference")
                or self.installer_state.get("image_override")
                or ""
            ).strip()
            parameter_args = [
                "--parameters",
                f"location={args['location']}",
                f"adminOid={args['admin_oid']}",
            ]
            if image_reference:
                parameter_args.append(f"saoImage={image_reference}")
            else:
                parameter_args.append("saoImageTag=latest")
            if name_suffix:
                parameter_args.append(f"nameSuffix={name_suffix}")
            return [
                format_az_command(
                    [
                        "deployment",
                        "group",
                        "validate",
                        "--name",
                        DEFAULT_DEPLOYMENT_NAME,
                        "--resource-group",
                        args["resource_group"],
                        "--template-file",
                        "/app/bicep/main.bicep",
                        *parameter_args,
                        "--output",
                        "json",
                    ],
                    host_os=host_os,
                ),
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
                        *parameter_args,
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
                        "containerapp",
                        "revision",
                        "list",
                        "--name",
                        "sao-app",
                        "--resource-group",
                        args["resource_group"],
                        "--all",
                        "--output",
                        "json",
                    ],
                    host_os=host_os,
                ),
                format_az_command(
                    [
                        "containerapp",
                        "replica",
                        "list",
                        "--name",
                        "sao-app",
                        "--resource-group",
                        args["resource_group"],
                        "--output",
                        "json",
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

        if name == "review_last_failure":
            resource_group = str(
                self.installer_state.get("resource_group") or ""
            ).strip()
            deployment_name = str(
                self.installer_state.get("deployment_name")
                or DEFAULT_DEPLOYMENT_NAME
            ).strip()
            if not resource_group:
                return []
            preview_commands = [
                format_az_command(
                    [
                        "deployment",
                        "group",
                        "show",
                        "--resource-group",
                        resource_group,
                        "--name",
                        deployment_name,
                        "--query",
                        "properties.error",
                        "--output",
                        "json",
                    ],
                    host_os=host_os,
                ),
                format_az_command(
                    [
                        "deployment",
                        "operation",
                        "group",
                        "list",
                        "--resource-group",
                        resource_group,
                        "--name",
                        deployment_name,
                        "--output",
                        "json",
                    ],
                    host_os=host_os,
                ),
            ]
            bundle = self.installer_state.get("last_failure_bundle") or {}
            failed_resource_type = str(
                bundle.get("failed_resource_type") or ""
            ).lower()
            issue_type = str(bundle.get("issue_type") or "").lower()
            if "keyvault" in failed_resource_type or "keyvault" in issue_type:
                preview_commands.append(
                    format_az_command(
                        [
                            "keyvault",
                            "list-deleted",
                            "--resource-type",
                            "vault",
                            "--output",
                            "json",
                        ],
                        host_os=host_os,
                    )
                )
            if (
                "containerapp" in failed_resource_type
                or "container_image" in issue_type
                or issue_type
                in {
                    "containerapp_revision_failed",
                    "containerapp_postgres_tls_mismatch",
                }
            ):
                preview_commands.extend(
                    [
                        format_az_command(
                            [
                                "deployment",
                                "group",
                                "show",
                                "--resource-group",
                                resource_group,
                                "--name",
                                "container-app",
                                "--query",
                                "properties.error",
                                "--output",
                                "json",
                            ],
                            host_os=host_os,
                        ),
                        format_az_command(
                            [
                                "deployment",
                                "operation",
                                "group",
                                "list",
                                "--resource-group",
                                resource_group,
                                "--name",
                                "container-app",
                                "--output",
                                "json",
                            ],
                            host_os=host_os,
                        ),
                        format_az_command(
                            [
                                "containerapp",
                                "show",
                                "--resource-group",
                                resource_group,
                                "--name",
                                "sao-app",
                                "--output",
                                "json",
                            ],
                            host_os=host_os,
                        ),
                        format_az_command(
                            [
                                "containerapp",
                                "revision",
                                "list",
                                "--resource-group",
                                resource_group,
                                "--name",
                                "sao-app",
                                "--all",
                                "--output",
                                "json",
                            ],
                            host_os=host_os,
                        ),
                        format_az_command(
                            [
                                "containerapp",
                                "replica",
                                "list",
                                "--resource-group",
                                resource_group,
                                "--name",
                                "sao-app",
                                "--output",
                                "json",
                            ],
                            host_os=host_os,
                        ),
                        format_az_command(
                            [
                                "containerapp",
                                "logs",
                                "show",
                                "--resource-group",
                                resource_group,
                                "--name",
                                "sao-app",
                                "--tail",
                                "50",
                                "--output",
                                "json",
                            ],
                            host_os=host_os,
                        ),
                        format_az_command(
                            [
                                "containerapp",
                                "logs",
                                "show",
                                "--resource-group",
                                resource_group,
                                "--name",
                                "sao-app",
                                "--type",
                                "system",
                                "--tail",
                                "50",
                                "--output",
                                "json",
                            ],
                            host_os=host_os,
                        ),
                    ]
                )
            return preview_commands

        if name == "apply_guided_fix":
            action = str(args.get("action") or "").strip()
            bundle = self.installer_state.get("last_failure_bundle") or {}
            resource_group = str(
                args.get("resource_group")
                or self.installer_state.get("resource_group")
                or ""
            ).strip()
            if action == "cleanup_resource_group" and resource_group:
                return self._build_preview_commands(
                    "delete_resource_group",
                    {"resource_group": resource_group},
                    host_os,
                )
            if action == "purge_deleted_key_vault":
                deleted_vault = bundle.get("deleted_vault") or {}
                vault_name = str(
                    deleted_vault.get("name")
                    or bundle.get("failed_resource_name")
                    or ""
                ).strip()
                location = str(
                    deleted_vault.get("location")
                    or self.installer_state.get("location")
                    or ""
                ).strip()
                preview_commands: list[str] = []
                if vault_name and location:
                    preview_commands.append(
                        format_az_command(
                            [
                                "keyvault",
                                "purge",
                                "--name",
                                vault_name,
                                "--location",
                                location,
                            ],
                            host_os=host_os,
                        )
                    )
                if resource_group and self.installer_state.get("admin_oid"):
                    preview_commands.extend(
                        self._build_preview_commands(
                            "provision_infrastructure",
                            {
                                "resource_group": resource_group,
                                "location": self.installer_state.get("location"),
                                "admin_oid": self.installer_state.get("admin_oid"),
                            },
                            host_os,
                        )
                    )
                return preview_commands
            if action == "retry_with_name_suffix":
                return self._build_preview_commands(
                    "provision_infrastructure",
                    {
                        "resource_group": resource_group,
                        "location": self.installer_state.get("location"),
                        "admin_oid": self.installer_state.get("admin_oid"),
                        "name_suffix": args.get("name_suffix")
                        or self.installer_state.get("name_suffix")
                        or self._suggest_name_suffix(),
                    },
                    host_os,
                )
            if action == "retry_with_image_override":
                return self._build_preview_commands(
                    "provision_infrastructure",
                    {
                        "resource_group": resource_group,
                        "location": self.installer_state.get("location"),
                        "admin_oid": self.installer_state.get("admin_oid"),
                        "image_reference": args.get("image_reference")
                        or bundle.get("image_reference")
                        or self.installer_state.get("image_override"),
                    },
                    host_os,
                )
            return []

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
                self.base_system_prompt
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
            "Browser access uses your organization's Entra ID. Azure also "
            "created a PostgreSQL admin credential for the managed database, "
            "and the runtime database connection is stored in Container Apps "
            "secrets."
        )
        if health_summary:
            print(f"Health check: {health_summary}")

    def _deployment_status_field(self, health_result: str, prefix: str) -> str:
        """Read one prefixed status line from the deployment health summary."""
        for line in health_result.splitlines():
            if line.startswith(prefix):
                return line[len(prefix) :].strip()
        return ""

    def _deployment_runtime_state(self, health_result: str) -> str:
        """Return the summarized runtime state from check_deployment_status."""
        return self._deployment_status_field(
            health_result, "Runtime state: "
        ).lower()

    def _deployment_is_ready(self, health_result: str) -> bool:
        """Return True when runtime verification says the app is ready."""
        return (
            self._deployment_status_field(health_result, "Ready: ").lower()
            == "true"
        )

    def _summarize_failed_operation(self, operation: dict[str, Any]) -> str:
        """Render one normalized ARM failure into a compact sentence."""
        resource_type = str(operation.get("resource_type") or "").strip()
        resource_name = str(operation.get("resource_name") or "").strip()
        provisioning_state = str(
            operation.get("provisioning_state") or ""
        ).strip()
        deployment_name = str(operation.get("deployment_name") or "").strip()
        status_messages = operation.get("status_messages", [])

        target_label = "/".join(
            part for part in (resource_type, resource_name) if part
        ) or "Deployment operation"
        if deployment_name:
            target_label = f"{target_label} via {deployment_name}"
        if provisioning_state:
            target_label = f"{target_label} ({provisioning_state})"
        if status_messages:
            target_label = f"{target_label}: {status_messages[0]}"
        return target_label

    def _flatten_failed_operations(
        self, deployment_diagnostics: dict[str, Any]
    ) -> list[dict[str, Any]]:
        """Flatten failed operations across the deployment tree."""
        operations: list[dict[str, Any]] = []
        deployment_name = str(
            deployment_diagnostics.get("deployment_name") or ""
        ).strip()
        for operation in deployment_diagnostics.get("failed_operations", []):
            if not isinstance(operation, dict):
                continue
            normalized = dict(operation)
            normalized["deployment_name"] = deployment_name
            operations.append(normalized)

        for nested in deployment_diagnostics.get("nested", []):
            if isinstance(nested, dict):
                operations.extend(self._flatten_failed_operations(nested))
        return operations

    def _first_nested_error(
        self, deployment_diagnostics: dict[str, Any]
    ) -> dict[str, Any] | None:
        """Return the first nested deployment that exposes an error payload."""
        for nested in deployment_diagnostics.get("nested", []):
            if not isinstance(nested, dict):
                continue
            if nested.get("error") is not None:
                return {
                    "deployment_name": nested.get("deployment_name"),
                    "error": nested.get("error"),
                }
            nested_match = self._first_nested_error(nested)
            if nested_match is not None:
                return nested_match
        return None

    def _select_failed_resource(
        self, failed_operations: list[dict[str, Any]]
    ) -> dict[str, str]:
        """Choose the most specific failed resource from the operation list."""
        for operation in failed_operations:
            resource_type = str(operation.get("resource_type") or "").strip()
            resource_name = str(operation.get("resource_name") or "").strip()
            if (
                resource_type
                and resource_name
                and resource_type.lower() != "microsoft.resources/deployments"
            ):
                return {
                    "type": resource_type,
                    "name": resource_name,
                    "deployment_name": str(
                        operation.get("deployment_name") or ""
                    ).strip(),
                }

        if failed_operations:
            first_operation = failed_operations[0]
            return {
                "type": str(first_operation.get("resource_type") or "").strip(),
                "name": str(first_operation.get("resource_name") or "").strip(),
                "deployment_name": str(
                    first_operation.get("deployment_name") or ""
                ).strip(),
            }

        return {"type": "", "name": "", "deployment_name": ""}

    def _extract_image_reference(
        self,
        container_app_diagnostics: dict[str, Any] | None,
        evidence_lines: list[str],
    ) -> str:
        """Pull the most likely image reference from app state or errors."""
        from tools.troubleshooting import DEFAULT_IMAGE_REFERENCE

        if isinstance(container_app_diagnostics, dict):
            app_payload = container_app_diagnostics.get("app")
            if isinstance(app_payload, dict):
                properties = app_payload.get("properties", {})
                if not isinstance(properties, dict):
                    properties = {}
                template = properties.get("template", {})
                if not isinstance(template, dict):
                    template = {}
                containers = template.get("containers", [])
                if isinstance(containers, list):
                    for container in containers:
                        if not isinstance(container, dict):
                            continue
                        image_reference = str(
                            container.get("image") or ""
                        ).strip()
                        if image_reference:
                            return image_reference

        combined_evidence = "\n".join(evidence_lines)
        image_match = re.search(
            r"([a-z0-9.-]+(?:/[a-z0-9._-]+)+:[A-Za-z0-9._-]+)",
            combined_evidence,
            flags=re.IGNORECASE,
        )
        if image_match:
            return image_match.group(1)

        image_override = str(
            self.installer_state.get("image_override") or ""
        ).strip()
        return image_override or DEFAULT_IMAGE_REFERENCE

    def _summarize_container_app_diagnostics(
        self, container_app_diagnostics: dict[str, Any] | None
    ) -> dict[str, Any] | None:
        """Trim container app diagnostics down to the fields we need."""
        if not isinstance(container_app_diagnostics, dict):
            return None

        app_payload = container_app_diagnostics.get("app")
        app_properties = (
            app_payload.get("properties", {})
            if isinstance(app_payload, dict)
            else {}
        )
        if not isinstance(app_properties, dict):
            app_properties = {}
        template = app_properties.get("template", {})
        if not isinstance(template, dict):
            template = {}
        containers = template.get("containers", [])
        image_reference = ""
        if isinstance(containers, list):
            for container in containers:
                if not isinstance(container, dict):
                    continue
                image_reference = str(container.get("image") or "").strip()
                if image_reference:
                    break

        revisions = container_app_diagnostics.get("revisions", [])
        revision_count = len(revisions) if isinstance(revisions, list) else 0
        latest_revision_name = str(
            container_app_diagnostics.get("latest_revision") or ""
        ).strip()
        latest_revision: dict[str, Any] = {}
        if isinstance(revisions, list):
            for revision in revisions:
                if not isinstance(revision, dict):
                    continue
                if (
                    latest_revision_name
                    and str(revision.get("name") or "").strip()
                    == latest_revision_name
                ):
                    latest_revision = revision
                    break
            if not latest_revision and revisions and isinstance(revisions[0], dict):
                latest_revision = revisions[0]

        latest_revision_properties = (
            latest_revision.get("properties", {})
            if isinstance(latest_revision, dict)
            else {}
        )
        if not isinstance(latest_revision_properties, dict):
            latest_revision_properties = {}

        replicas = container_app_diagnostics.get("replicas", [])
        latest_replica = (
            replicas[0] if isinstance(replicas, list) and replicas else {}
        )
        latest_replica_properties = (
            latest_replica.get("properties", {})
            if isinstance(latest_replica, dict)
            else {}
        )
        if not isinstance(latest_replica_properties, dict):
            latest_replica_properties = {}
        replica_containers = latest_replica_properties.get("containers", [])
        if not isinstance(replica_containers, list):
            replica_containers = []
        first_replica_container = (
            replica_containers[0] if replica_containers else {}
        )
        if not isinstance(first_replica_container, dict):
            first_replica_container = {}

        return {
            "provisioning_state": app_properties.get("provisioningState"),
            "image": image_reference,
            "latest_revision": latest_revision_name
            or container_app_diagnostics.get("latest_revision"),
            "revision_count": revision_count,
            "revision_health": latest_revision_properties.get("healthState"),
            "revision_state": latest_revision_properties.get("runningState"),
            "revision_details": latest_revision_properties.get(
                "runningStateDetails"
            )
            or latest_revision_properties.get("provisioningError"),
            "replica_state": latest_replica_properties.get("runningState"),
            "replica_details": latest_replica_properties.get(
                "runningStateDetails"
            )
            or first_replica_container.get("runningStateDetails"),
            "replica_restart_count": first_replica_container.get(
                "restartCount"
            ),
            "app_logs": self._extract_error_messages(
                container_app_diagnostics.get("app_logs")
            )[:5],
            "system_logs": self._extract_error_messages(
                container_app_diagnostics.get("system_logs")
            )[:5],
            "collection_errors": self._extract_error_messages(
                container_app_diagnostics.get("collection_errors")
            )[:3],
        }

    def _extract_container_app_evidence(
        self, container_app_diagnostics: dict[str, Any] | None
    ) -> list[str]:
        """Pull high-signal runtime evidence out of Container App diagnostics."""
        summary = self._summarize_container_app_diagnostics(
            container_app_diagnostics
        )
        if not isinstance(summary, dict):
            return []

        evidence: list[str] = []
        for key in (
            "revision_health",
            "revision_state",
            "revision_details",
            "replica_state",
            "replica_details",
        ):
            value = str(summary.get(key) or "").strip()
            if value:
                evidence.append(value)

        restart_count = summary.get("replica_restart_count")
        if restart_count not in (None, ""):
            evidence.append(f"Replica restart count: {restart_count}")

        for key in ("app_logs", "system_logs", "collection_errors"):
            evidence.extend(self._extract_error_messages(summary.get(key)))

        return self._dedupe_lines(evidence)

    def _clear_last_failure_bundle(self) -> None:
        """Clear troubleshooting state after a successful recovery."""
        self.installer_state["last_failure_bundle"] = None
        self.installer_state["troubleshooting_active"] = False

    def _store_last_failure_bundle(self, bundle: dict[str, Any]) -> None:
        """Persist the latest failure bundle for later review."""
        self.installer_state["last_failure_bundle"] = bundle
        self.installer_state["troubleshooting_active"] = True

    def _collect_failure_bundle(
        self,
        resource_group: str,
        deployment_name: str,
        host_os: str,
        initial_failure: str | None = None,
    ) -> dict[str, Any]:
        """Collect structured diagnostics for the current deployment failure."""
        from tools.azure import (
            collect_container_app_diagnostics,
            collect_group_deployment_diagnostics,
            list_deleted_key_vaults,
        )
        from tools.troubleshooting import build_troubleshooting_response

        deployment_diagnostics = collect_group_deployment_diagnostics(
            resource_group=resource_group,
            deployment_name=deployment_name,
            host_os=host_os,
        )
        failed_operations = self._flatten_failed_operations(
            deployment_diagnostics
        )
        nested_error = self._first_nested_error(deployment_diagnostics)
        top_level_error = deployment_diagnostics.get("error")

        evidence_lines = self._extract_error_messages(initial_failure)
        evidence_lines.extend(self._extract_error_messages(top_level_error))
        if nested_error is not None:
            evidence_lines.extend(
                self._extract_error_messages(nested_error.get("error"))
            )

        for operation in failed_operations:
            evidence_lines.append(self._summarize_failed_operation(operation))
            for status_message in operation.get("status_messages", []):
                evidence_lines.extend(
                    self._extract_error_messages(status_message)
                )

        collection_errors = deployment_diagnostics.get("collection_errors", [])
        if isinstance(collection_errors, list):
            evidence_lines.extend(
                str(item).strip() for item in collection_errors if str(item).strip()
            )

        evidence_lines = self._dedupe_lines(evidence_lines)
        failed_resource = self._select_failed_resource(failed_operations)
        combined_evidence = "\n".join(evidence_lines)

        key_vault_name = (
            failed_resource.get("name")
            if str(failed_resource.get("type") or "").lower().startswith(
                "microsoft.keyvault/"
            )
            else self._extract_key_vault_name(combined_evidence)
        )
        deleted_vault_match: dict[str, str] | None = None
        if key_vault_name:
            deleted_vaults_result = list_deleted_key_vaults(host_os=host_os)
            if (
                "COMMAND FAILED" not in deleted_vaults_result
                and "COMMAND CANCELLED" not in deleted_vaults_result
            ):
                parsed_deleted_vaults = self._try_parse_json(
                    deleted_vaults_result
                )
                deleted_vault_match = self._find_deleted_key_vault(
                    parsed_deleted_vaults, key_vault_name
                )

        lowered_evidence = combined_evidence.lower()
        should_check_container_app = (
            str(failed_resource.get("type") or "").lower().startswith(
                "microsoft.app/containerapps"
            )
            or "ghcr.io" in lowered_evidence
            or "containerapp" in lowered_evidence
            or "revision" in lowered_evidence
        )
        raw_container_app_diagnostics = (
            collect_container_app_diagnostics(
                resource_group=resource_group,
                host_os=host_os,
            )
            if should_check_container_app
            else None
        )
        evidence_lines.extend(
            self._extract_container_app_evidence(raw_container_app_diagnostics)
        )
        evidence_lines = self._dedupe_lines(evidence_lines)
        combined_evidence = "\n".join(evidence_lines)
        container_app_summary = self._summarize_container_app_diagnostics(
            raw_container_app_diagnostics
        )
        image_reference = self._extract_image_reference(
            raw_container_app_diagnostics, evidence_lines
        )

        troubleshooting = build_troubleshooting_response(
            {
                "resource_group": resource_group,
                "deployment_name": deployment_name,
                "location": self.installer_state.get("location"),
                "failed_resource_type": failed_resource.get("type"),
                "failed_resource_name": failed_resource.get("name"),
                "raw_error": combined_evidence,
                "top_level_error": top_level_error,
                "nested_error": nested_error.get("error")
                if nested_error is not None
                else None,
                "evidence": evidence_lines,
                "deleted_vault": deleted_vault_match,
                "image_reference": image_reference,
                "host_os": host_os,
            }
        )

        bundle = {
            "resource_group": resource_group,
            "deployment_name": deployment_name,
            "location": self.installer_state.get("location"),
            "top_level_error": top_level_error,
            "nested_error": nested_error.get("error")
            if nested_error is not None
            else None,
            "nested_deployment_name": nested_error.get("deployment_name")
            if nested_error is not None
            else None,
            "nested_deployments": deployment_diagnostics.get("nested", []),
            "failed_operations": failed_operations[:8],
            "failed_resource": failed_resource,
            "failed_resource_type": failed_resource.get("type"),
            "failed_resource_name": failed_resource.get("name"),
            "issue_type": troubleshooting.get("issue_type"),
            "diagnosis": troubleshooting.get("diagnosis"),
            "evidence": evidence_lines[:8],
            "suggested_actions": troubleshooting.get("guided_actions", []),
            "manual_commands": troubleshooting.get("manual_commands", []),
            "safe_to_auto_apply": troubleshooting.get(
                "safe_to_auto_apply", []
            ),
            "deleted_vault": deleted_vault_match,
            "image_reference": image_reference,
            "container_app": container_app_summary,
            "raw_error": combined_evidence,
        }
        self._store_last_failure_bundle(bundle)
        return bundle

    def _describe_guided_action(
        self, action: str, bundle: dict[str, Any] | None = None
    ) -> str:
        """Render a guided fix action in operator-facing language."""
        bundle = bundle or {}
        if action == "purge_deleted_key_vault":
            vault_name = str(
                (bundle.get("deleted_vault") or {}).get("name") or ""
            ).strip()
            if vault_name:
                return f"Purge the deleted Key Vault {vault_name} and retry."
            return "Purge the deleted Key Vault and retry."
        if action == "retry_with_name_suffix":
            return "Retry with a short unique suffix so Azure gets fresh names."
        if action == "retry_with_image_override":
            issue_type = str(bundle.get("issue_type") or "").strip().lower()
            if issue_type == "container_image_ghcr_private":
                return (
                    "Make the GHCR package public in GitHub, or retry with an "
                    "alternate image reference that Azure can pull."
                )
            if issue_type == "containerapp_postgres_tls_mismatch":
                return (
                    "Rebuild and redeploy the SAO app image with SQLx TLS "
                    "enabled, then retry with that image."
                )
            return "Retry with an alternate image reference that Azure can pull."
        if action == "cleanup_resource_group":
            return "Clean up the SAO test resource group and start fresh."
        return action.replace("_", " ")

    def _review_last_failure(self, host_os: str) -> str:
        """Return the cached failure bundle, or collect it if needed."""
        bundle = self.installer_state.get("last_failure_bundle")
        if isinstance(bundle, dict) and bundle:
            return json.dumps(bundle, indent=2)

        resource_group = str(self.installer_state.get("resource_group") or "").strip()
        deployment_name = str(
            self.installer_state.get("deployment_name") or DEFAULT_DEPLOYMENT_NAME
        ).strip()
        if not resource_group:
            return json.dumps(
                {
                    "issue_type": "unknown",
                    "diagnosis": "No bootstrap failure has been recorded yet.",
                    "evidence": [],
                    "guided_actions": [],
                    "manual_commands": [],
                    "safe_to_auto_apply": [],
                },
                indent=2,
            )

        bundle = self._collect_failure_bundle(
            resource_group=resource_group,
            deployment_name=deployment_name,
            host_os=host_os,
        )
        return json.dumps(bundle, indent=2)

    def _reset_after_cleanup(self) -> None:
        """Clear state that only applies to an active deployment."""
        self.installer_state["resource_group"] = None
        self.installer_state["location"] = None
        self.installer_state["deployment_name"] = None
        self.installer_state["name_suffix"] = ""
        self.installer_state["image_override"] = None
        self.installer_state["sao_endpoint"] = None
        self._clear_last_failure_bundle()

    def _retry_current_provisioning(self, host_os: str) -> str:
        """Retry provisioning with the current installer state."""
        resource_group = str(self.installer_state.get("resource_group") or "").strip()
        location = str(self.installer_state.get("location") or "").strip()
        admin_oid = str(self.installer_state.get("admin_oid") or "").strip()
        if not resource_group or not location or not admin_oid:
            return (
                "COMMAND FAILED: I do not have enough deployment context to "
                "retry yet. I need the resource group, location, and admin OID."
            )

        retry_args = {
            "resource_group": resource_group,
            "location": location,
            "admin_oid": admin_oid,
        }
        image_override = str(
            self.installer_state.get("image_override") or ""
        ).strip()
        if image_override:
            retry_args["image_reference"] = image_override
        return self._run_provisioning_with_polling(retry_args, host_os)

    def _apply_guided_fix(self, args: dict[str, Any], host_os: str) -> str:
        """Apply a supported recovery action and retry when appropriate."""
        from tools.azure import delete_resource_group, purge_deleted_key_vault

        action = str(args.get("action") or "").strip()
        bundle = self.installer_state.get("last_failure_bundle") or {}
        if not action:
            return "COMMAND FAILED: apply_guided_fix requires an action."

        if action == "purge_deleted_key_vault":
            deleted_vault = bundle.get("deleted_vault") or {}
            vault_name = str(
                deleted_vault.get("name")
                or bundle.get("failed_resource_name")
                or ""
            ).strip()
            location = str(
                deleted_vault.get("location")
                or self.installer_state.get("location")
                or ""
            ).strip()
            if not vault_name or not location:
                return (
                    "COMMAND FAILED: I could not find a deleted Key Vault name "
                    "and location to purge."
                )
            purge_result = purge_deleted_key_vault(
                vault_name, location, host_os=host_os
            )
            if "COMMAND FAILED" in purge_result:
                return purge_result
            self.installer_state["name_suffix"] = ""
            retry_result = self._retry_current_provisioning(host_os)
            return f"{purge_result}\n\nRetry result:\n{retry_result}"

        if action == "retry_with_name_suffix":
            name_suffix = self._normalize_name_suffix(args.get("name_suffix"))
            if not name_suffix:
                name_suffix = self._suggest_name_suffix()
            self.installer_state["name_suffix"] = name_suffix
            retry_result = self._retry_current_provisioning(host_os)
            return (
                f"Retrying the deployment with unique suffix {name_suffix}.\n\n"
                f"{retry_result}"
            )

        if action == "retry_with_image_override":
            image_reference = str(args.get("image_reference") or "").strip()
            if not image_reference:
                image_reference = str(bundle.get("image_reference") or "").strip()
            if not image_reference:
                return (
                    "COMMAND FAILED: retry_with_image_override requires an "
                    "image_reference."
                )
            self.installer_state["image_override"] = image_reference
            retry_result = self._retry_current_provisioning(host_os)
            return (
                "Retrying the deployment with image override "
                f"{image_reference}.\n\n{retry_result}"
            )

        if action == "cleanup_resource_group":
            resource_group = str(
                args.get("resource_group")
                or self.installer_state.get("resource_group")
                or ""
            ).strip()
            if not resource_group:
                return (
                    "COMMAND FAILED: cleanup_resource_group requires a resource group."
                )
            cleanup_result = delete_resource_group(
                resource_group, host_os=host_os
            )
            if "COMMAND FAILED" in cleanup_result:
                return cleanup_result
            self._reset_after_cleanup()
            return cleanup_result

        return f"COMMAND FAILED: Unsupported guided action {action}."

    def _format_provisioning_failure_message(
        self,
        deployment_name: str,
        context_label: str,
        diagnostics: dict[str, Any],
    ) -> str:
        """Render a concise but actionable provisioning failure summary."""
        lines = [
            "COMMAND FAILED: "
            f"{context_label} for Azure deployment {deployment_name} did not succeed."
        ]

        diagnosis = str(diagnostics.get("diagnosis") or "").strip()
        if diagnosis:
            lines.append(f"Likely issue: {diagnosis}")

        issue_type = str(diagnostics.get("issue_type") or "").strip().lower()
        image_reference = str(
            diagnostics.get("image_reference") or ""
        ).strip()
        if issue_type == "container_image_ghcr_private":
            image_label = image_reference or "the configured GHCR image"
            lines.append(
                "Recommended fix: set the GitHub Container Registry package "
                f"backing {image_label} to Public, then retry the deployment."
            )
        elif issue_type == "containerapp_postgres_tls_mismatch":
            lines.append(
                "Recommended fix: rebuild the SAO application image with SQLx "
                "Tokio+Rustls TLS support enabled, publish it, and redeploy "
                "that image."
            )

        failed_resource = diagnostics.get("failed_resource") or {}
        resource_type = str(failed_resource.get("type") or "").strip()
        resource_name = str(failed_resource.get("name") or "").strip()
        if resource_type or resource_name:
            resource_label = "/".join(
                part for part in (resource_type, resource_name) if part
            )
            lines.append(f"Failing resource: {resource_label}")

        nested_deployment_name = str(
            diagnostics.get("nested_deployment_name") or ""
        ).strip()
        if nested_deployment_name:
            lines.append(f"Nested deployment: {nested_deployment_name}")

        evidence = diagnostics.get("evidence", [])
        if evidence:
            lines.append("Azure reported:")
            for line in evidence[:3]:
                lines.append(f"- {line}")

        suggested_actions = diagnostics.get("suggested_actions", [])
        if suggested_actions:
            lines.append("Suggested recovery:")
            for action in suggested_actions[:3]:
                lines.append(
                    f"- {self._describe_guided_action(action, diagnostics)}"
                )

        return "\n".join(lines)

    def _run_provisioning_with_polling(
        self, args: dict[str, Any], host_os: str
    ) -> str:
        """Start the Azure deployment and poll until it reaches a terminal state."""
        from tools.azure import (
            check_deployment_status,
            get_group_deployment_endpoint,
            get_group_deployment_status,
            start_infrastructure_provisioning,
            validate_infrastructure_provisioning,
        )

        resource_group = args["resource_group"]
        location = args["location"]
        admin_oid = args["admin_oid"]
        deployment_name = DEFAULT_DEPLOYMENT_NAME
        image_reference = str(
            args.get("image_reference")
            or self.installer_state.get("image_override")
            or ""
        ).strip()

        self.installer_state["deployment_name"] = deployment_name
        self.installer_state["resource_group"] = resource_group
        self.installer_state["location"] = location
        self.installer_state["admin_oid"] = admin_oid
        if image_reference:
            self.installer_state["image_override"] = image_reference

        name_suffix = self._normalize_name_suffix(
            self.installer_state.get("name_suffix")
        )
        runtime_check_started_at: float | None = None

        validation_result = validate_infrastructure_provisioning(
            resource_group=resource_group,
            location=location,
            admin_oid=admin_oid,
            host_os=host_os,
            deployment_name=deployment_name,
            name_suffix=name_suffix,
            sao_image=image_reference,
        )
        if (
            "COMMAND FAILED" in validation_result
            or "COMMAND CANCELLED" in validation_result
        ):
            diagnostics = self._collect_failure_bundle(
                resource_group=resource_group,
                deployment_name=deployment_name,
                host_os=host_os,
                initial_failure=validation_result,
            )
            return self._format_provisioning_failure_message(
                deployment_name=deployment_name,
                context_label="Provisioning validation",
                diagnostics=diagnostics,
            )

        start_result = start_infrastructure_provisioning(
            resource_group=resource_group,
            location=location,
            admin_oid=admin_oid,
            host_os=host_os,
            deployment_name=deployment_name,
            name_suffix=name_suffix,
            sao_image=image_reference,
        )
        if "COMMAND FAILED" in start_result or "COMMAND CANCELLED" in start_result:
            diagnostics = self._collect_failure_bundle(
                resource_group=resource_group,
                deployment_name=deployment_name,
                host_os=host_os,
                initial_failure=start_result,
            )
            return self._format_provisioning_failure_message(
                deployment_name=deployment_name,
                context_label="Provisioning start",
                diagnostics=diagnostics,
            )

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
                if "COMMAND CANCELLED" in health_result:
                    return health_result
                if "COMMAND FAILED" not in health_result:
                    self._update_state(
                        "check_deployment_status",
                        {"resource_group": resource_group},
                        health_result,
                    )
                    if self.installer_state["sao_endpoint"]:
                        endpoint = self.installer_state["sao_endpoint"]

                if self._deployment_is_ready(health_result):
                    self._clear_last_failure_bundle()
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
                    if name_suffix:
                        result["name_suffix"] = name_suffix
                    if image_reference:
                        result["image_reference"] = image_reference
                    return json.dumps(result, indent=2)

                runtime_state = self._deployment_runtime_state(health_result)
                if runtime_state == "failed":
                    diagnostics = self._collect_failure_bundle(
                        resource_group=resource_group,
                        deployment_name=deployment_name,
                        host_os=host_os,
                        initial_failure=health_result,
                    )
                    return self._format_provisioning_failure_message(
                        deployment_name=deployment_name,
                        context_label=(
                            "Azure runtime verification"
                            " (ARM deployment succeeded but the app revision failed)"
                        ),
                        diagnostics=diagnostics,
                    )

                runtime_check_started_at = (
                    runtime_check_started_at or time.monotonic()
                )
                runtime_elapsed = self._format_elapsed(
                    time.monotonic() - runtime_check_started_at
                )
                print(
                    "\nAzure finished provisioning the resources, but the SAO "
                    "application is still warming up "
                    f"({runtime_elapsed} runtime verification elapsed)."
                )
                print(health_result)

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
                        "runtime_health": health_result,
                        "elapsed": elapsed,
                        "admin_oid": admin_oid,
                        "name_suffix": name_suffix,
                        "image_reference": image_reference,
                    },
                )
                time.sleep(POLL_INTERVAL_SECONDS)
                continue

            if state in {"Failed", "Canceled"}:
                diagnostics = self._collect_failure_bundle(
                    resource_group=resource_group,
                    deployment_name=deployment_name,
                    host_os=host_os,
                    initial_failure=status_result,
                )
                return self._format_provisioning_failure_message(
                    deployment_name=deployment_name,
                    context_label=(
                        "Azure deployment failure"
                        f" ({state} at {parsed_status.get('timestamp', 'unknown time')})"
                    ),
                    diagnostics=diagnostics,
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
                    "name_suffix": name_suffix,
                    "image_reference": image_reference,
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
        all_read_only = all(
            policy.risk_class == "read" for policy in selected_policies
        )
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
        print(self._phase_intro_text(phase))
        if preview_commands:
            print("Planned Azure CLI commands:")
            for command in preview_commands:
                print(f"  - {command}")
        if batch_read_only:
            print(
                "This is a read-only batch. One approval will cover every "
                "discovery and permission check listed above."
            )
        elif all_read_only:
            print(
                "This is a read-only review. I will still pause for approval "
                "before I query Azure."
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
                approval_prompt = (
                    "Approve this read-only review? (y/n) "
                    if policy.risk_class == "read"
                    else f"Approve {policy.preview_text}? (y/n) "
                )
                approved = self._confirm_yes_no(approval_prompt)
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
                name_suffix=(
                    args.get("name_suffix")
                    or self.installer_state.get("name_suffix")
                ),
                sao_image=(
                    args.get("image_reference")
                    or self.installer_state.get("image_override")
                ),
            ),
            "check_deployment_status": lambda: check_deployment_status(
                args["resource_group"], host_os=host_os
            ),
            "review_last_failure": lambda: self._review_last_failure(host_os),
            "apply_guided_fix": lambda: self._apply_guided_fix(
                args, host_os
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
            self._reset_after_cleanup()

        elif tool_name == "review_last_failure" and parsed:
            self._store_last_failure_bundle(parsed)

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
