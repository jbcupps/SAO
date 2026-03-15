"""SAO Installer — Conversation manager and tool dispatch."""

import json
import os
import re
import time
from dataclasses import dataclass
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
        "lead_in": "Before I run this custom Azure command:",
        "intro": (
            "I want to make the exact action and its impact clear because it "
            "falls outside the dedicated installer tools."
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


@dataclass(frozen=True)
class ProvisioningRecoveryDecision:
    """Result of a provisioning recovery analysis."""

    action: str
    name_suffix: str | None = None
    message: str | None = None


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
            "name_suffix": "",
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

    def _phase_intro_text(self, phase: str) -> str:
        """Render a natural-sounding phase introduction."""
        phase_detail = PHASE_DETAILS[phase]
        lead_in = phase_detail.get("lead_in", "Here's what I'm about to do and why:")
        intro = phase_detail["intro"]
        if lead_in.endswith(":"):
            return f"{lead_in} {intro}"
        return f"{lead_in} {intro}"

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

    def _analyze_provisioning_failure(
        self,
        resource_group: str,
        deployment_name: str,
        host_os: str,
        initial_failure: str | None = None,
    ) -> dict[str, Any]:
        """Collect Azure deployment diagnostics and infer likely recovery paths."""
        from tools.azure import (
            get_group_deployment_error,
            list_deleted_key_vaults,
            list_group_deployment_operations,
        )

        evidence_lines = self._extract_error_messages(initial_failure)

        deployment_error_result = get_group_deployment_error(
            resource_group=resource_group,
            deployment_name=deployment_name,
            host_os=host_os,
        )
        if (
            "COMMAND FAILED" not in deployment_error_result
            and "COMMAND CANCELLED" not in deployment_error_result
            and deployment_error_result.strip()
            and deployment_error_result.strip() != "null"
        ):
            parsed_deployment_error = (
                self._try_parse_json(deployment_error_result)
                or deployment_error_result
            )
            evidence_lines.extend(
                self._extract_error_messages(parsed_deployment_error)
            )

        operations_result = list_group_deployment_operations(
            resource_group=resource_group,
            deployment_name=deployment_name,
            host_os=host_os,
        )
        failed_operations: list[str] = []
        if (
            "COMMAND FAILED" not in operations_result
            and "COMMAND CANCELLED" not in operations_result
        ):
            parsed_operations = self._try_parse_json(operations_result)
            failed_operations = self._extract_failed_operation_summaries(
                parsed_operations
            )
            evidence_lines.extend(failed_operations)

        evidence_lines = self._dedupe_lines(evidence_lines)
        combined_evidence = "\n".join(evidence_lines)
        lowered_evidence = combined_evidence.lower()
        key_vault_name = self._extract_key_vault_name(combined_evidence)

        soft_delete_markers = (
            "soft-delete",
            "soft delete",
            "deleted but recoverable",
            "scheduled for deletion",
            "recoverable deleted",
        )
        conflict_markers = (
            "already exists",
            "already in use",
            "conflict",
            "is not available",
            "reserved",
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

        soft_delete_collision = any(
            marker in lowered_evidence for marker in soft_delete_markers
        ) or deleted_vault_match is not None
        name_conflict = (
            key_vault_name is not None
            and any(marker in lowered_evidence for marker in conflict_markers)
        )

        suggestions: list[str] = []
        if deleted_vault_match is not None:
            location = deleted_vault_match["location"] or self.installer_state[
                "location"
            ]
            suggestions.append(
                "This looks like a soft-delete collision for Key Vault "
                f"{deleted_vault_match['name']} in {location}."
            )
            suggestions.append(
                "I can purge the deleted vault and retry, or keep it and use a "
                "short suffix to generate a new global name."
            )
        elif name_conflict and key_vault_name:
            suggestions.append(
                f"The deployment is colliding with the global Key Vault name {key_vault_name}."
            )
            suggestions.append(
                "A short suffix retry will generate a fresh set of resource names."
            )
        else:
            suggestions.append(
                "The deployment operations identify the first failing Azure resource."
            )
            suggestions.append(
                "If this was a test run, cleanup is the safest reset before retrying."
            )

        return {
            "evidence": evidence_lines[:6],
            "failed_operations": failed_operations[:4],
            "soft_delete_collision": soft_delete_collision,
            "deleted_vault": deleted_vault_match,
            "name_conflict": name_conflict,
            "key_vault_name": key_vault_name,
            "suggestions": suggestions,
        }

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

        evidence = diagnostics.get("evidence", [])
        if evidence:
            lines.append("Azure reported:")
            for line in evidence[:3]:
                lines.append(f"- {line}")

        failed_operations = diagnostics.get("failed_operations", [])
        if failed_operations:
            lines.append("Failing operations:")
            for line in failed_operations[:2]:
                lines.append(f"- {line}")

        suggestions = diagnostics.get("suggestions", [])
        if suggestions:
            lines.append("Suggested recovery:")
            for line in suggestions[:2]:
                lines.append(f"- {line}")

        return "\n".join(lines)

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
            name_suffix = self._normalize_name_suffix(
                args.get("name_suffix")
                or self.installer_state.get("name_suffix")
            )
            parameter_args = [
                "--parameters",
                f"location={args['location']}",
                f"adminOid={args['admin_oid']}",
                "saoImageTag=latest",
            ]
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

    def _offer_preflight_recovery(
        self,
        diagnostics: dict[str, Any],
        resource_group: str,
        location: str,
        host_os: str,
    ) -> ProvisioningRecoveryDecision:
        """Offer safe retry options for preflight naming collisions."""
        from tools.azure import format_az_command, purge_deleted_key_vault

        deleted_vault = diagnostics.get("deleted_vault")
        key_vault_name = diagnostics.get("key_vault_name")

        if deleted_vault is not None:
            vault_name = deleted_vault["name"]
            purge_location = deleted_vault["location"] or location
            print("\nTroubleshooting:")
            print(
                "This looks like a soft-delete collision — the deployment is "
                f"trying to reuse Key Vault name {vault_name}, and Azure still "
                "has that name reserved."
            )
            print("Recovery command I can run for you:")
            print(
                "  - "
                + format_az_command(
                    [
                        "keyvault",
                        "purge",
                        "--name",
                        vault_name,
                        "--location",
                        purge_location,
                    ],
                    host_os=host_os,
                )
            )
            if self._confirm_yes_no(
                "This looks like a soft-delete collision — would you like me "
                "to purge it and continue? (y/n) "
            ):
                print(f"\nPurging deleted Key Vault {vault_name}...")
                purge_result = purge_deleted_key_vault(
                    vault_name,
                    purge_location,
                    host_os=host_os,
                )
                print(purge_result)
                if "COMMAND FAILED" in purge_result:
                    return ProvisioningRecoveryDecision(
                        action="fail",
                        message=purge_result,
                    )
                self.installer_state["name_suffix"] = ""
                return ProvisioningRecoveryDecision(action="retry")

        if diagnostics.get("name_conflict") and key_vault_name:
            suggested_suffix = self._suggest_name_suffix()
            print("\nTroubleshooting:")
            print(
                f"Azure is reporting a name collision for Key Vault {key_vault_name}."
            )
            print(
                "If that name came from an old test environment, cleanup is a "
                "good reset. If you want to keep moving now, I can retry with a "
                f"short suffix like {suggested_suffix}."
            )
            if self._confirm_yes_no(
                f"Retry this deployment with suffix {suggested_suffix}? (y/n) "
            ):
                self.installer_state["name_suffix"] = suggested_suffix
                return ProvisioningRecoveryDecision(
                    action="retry",
                    name_suffix=suggested_suffix,
                )

        return ProvisioningRecoveryDecision(
            action="fail",
            message=self._format_provisioning_failure_message(
                DEFAULT_DEPLOYMENT_NAME,
                "Provisioning preflight",
                diagnostics,
            ),
        )

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

        self.installer_state["deployment_name"] = deployment_name
        self.installer_state["resource_group"] = resource_group
        self.installer_state["location"] = location
        self.installer_state["admin_oid"] = admin_oid

        attempt_count = 0
        while attempt_count < 3:
            attempt_count += 1
            name_suffix = self._normalize_name_suffix(
                self.installer_state.get("name_suffix")
            )

            validation_result = validate_infrastructure_provisioning(
                resource_group=resource_group,
                location=location,
                admin_oid=admin_oid,
                host_os=host_os,
                deployment_name=deployment_name,
                name_suffix=name_suffix,
            )
            if (
                "COMMAND FAILED" in validation_result
                or "COMMAND CANCELLED" in validation_result
            ):
                diagnostics = self._analyze_provisioning_failure(
                    resource_group=resource_group,
                    deployment_name=deployment_name,
                    host_os=host_os,
                    initial_failure=validation_result,
                )
                recovery = self._offer_preflight_recovery(
                    diagnostics=diagnostics,
                    resource_group=resource_group,
                    location=location,
                    host_os=host_os,
                )
                if recovery.action == "retry":
                    continue
                return recovery.message or validation_result

            start_result = start_infrastructure_provisioning(
                resource_group=resource_group,
                location=location,
                admin_oid=admin_oid,
                host_os=host_os,
                deployment_name=deployment_name,
                name_suffix=name_suffix,
            )
            if (
                "COMMAND FAILED" in start_result
                or "COMMAND CANCELLED" in start_result
            ):
                diagnostics = self._analyze_provisioning_failure(
                    resource_group=resource_group,
                    deployment_name=deployment_name,
                    host_os=host_os,
                    initial_failure=start_result,
                )
                recovery = self._offer_preflight_recovery(
                    diagnostics=diagnostics,
                    resource_group=resource_group,
                    location=location,
                    host_os=host_os,
                )
                if recovery.action == "retry":
                    continue
                return recovery.message or start_result

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
                    if name_suffix:
                        result["name_suffix"] = name_suffix
                    return json.dumps(result, indent=2)

                if state in {"Failed", "Canceled"}:
                    diagnostics = self._analyze_provisioning_failure(
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

                stage_message = self._infer_provisioning_stage(
                    resource_group, host_os
                )
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
                    },
                )
                time.sleep(POLL_INTERVAL_SECONDS)

        return (
            "COMMAND FAILED: Provisioning hit repeated name-collision recovery "
            "attempts. Cleanup the test environment or retry with a fresh "
            "resource group."
        )

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
                name_suffix=(
                    args.get("name_suffix")
                    or self.installer_state.get("name_suffix")
                ),
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
            self.installer_state["name_suffix"] = ""
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
