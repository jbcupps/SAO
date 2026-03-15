"""SAO Installer — Conversation manager and tool dispatch."""

import json
import os
from pathlib import Path

import anthropic

TOOLS = [
    {
        "name": "az_login",
        "description": (
            "Initiate Azure device code login flow. Returns the device code "
            "URL and instructions for the user."
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
            "Get the currently signed-in Azure user's OID, UPN, and display name."
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
            "List Azure subscriptions the signed-in user has access to."
        ),
        "input_schema": {
            "type": "object",
            "properties": {},
            "required": [],
        },
    },
    {
        "name": "set_subscription",
        "description": "Set the active Azure subscription.",
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
            "Verify the user has required Azure and Entra permissions for "
            "SAO deployment. Checks subscription role, resource provider "
            "registration, and Graph API access."
        ),
        "input_schema": {
            "type": "object",
            "properties": {},
            "required": [],
        },
    },
    {
        "name": "create_resource_group",
        "description": "Create an Azure resource group for SAO.",
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
        "name": "provision_infrastructure",
        "description": (
            "Deploy the SAO Bicep template to the active subscription and "
            "resource group. Creates PostgreSQL, Key Vault, Container Apps "
            "environment, and the SAO container."
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
            "Check if the SAO container is running and healthy. "
            "Returns the app URL and health check result."
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
            "Run an arbitrary az CLI command. Use only when the specific "
            "tools above don't cover the needed operation. The command is "
            "visible to the user."
        ),
        "input_schema": {
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": (
                        "The full az CLI command to execute (without 'az' prefix)"
                    ),
                }
            },
            "required": ["command"],
        },
    },
]


class InstallerAgent:
    """Claude-powered conversational agent for SAO installation."""

    def __init__(self, provider: str, api_key: str):
        self.client = anthropic.Anthropic(api_key=api_key)
        self.model = "claude-sonnet-4-20250514"
        self.conversation: list[dict] = []
        self.system_prompt = self._load_system_prompt()
        self.installer_state = {
            "admin_oid": None,
            "admin_upn": None,
            "subscription_id": None,
            "resource_group": None,
            "location": None,
            "sao_endpoint": None,
        }

    def verify_connection(self) -> bool:
        """Quick API ping to verify the key works."""
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
        # Kick off the conversation — agent speaks first
        self.conversation.append(
            {
                "role": "user",
                "content": (
                    "I just connected. Please introduce yourself and "
                    "start the installation process."
                ),
            }
        )

        while True:
            response = self.client.messages.create(
                model=self.model,
                max_tokens=4096,
                system=self.system_prompt,
                tools=TOOLS,
                messages=self.conversation,
            )

            # Process response content blocks
            assistant_content = response.content
            self.conversation.append(
                {"role": "assistant", "content": assistant_content}
            )

            tool_results = []
            for block in assistant_content:
                if block.type == "text":
                    print(f"\n{block.text}")
                elif block.type == "tool_use":
                    print(f"\n  [executing: {block.name}...]")
                    result = self._dispatch_tool(block.name, block.input)
                    tool_results.append(
                        {
                            "type": "tool_result",
                            "tool_use_id": block.id,
                            "content": result,
                        }
                    )

            if tool_results:
                self.conversation.append(
                    {"role": "user", "content": tool_results}
                )
                continue  # Let the agent process tool results

            if response.stop_reason == "end_turn":
                # Agent finished speaking, wait for user input
                user_input = input("\nYou: ").strip()
                if user_input.lower() in ("exit", "quit", "q"):
                    print("Installation cancelled.")
                    break
                self.conversation.append(
                    {"role": "user", "content": user_input}
                )

    def _dispatch_tool(self, name: str, args: dict) -> str:
        """Route tool calls to implementations."""
        host_os = os.environ.get("HOST_OS", "windows")
        os.environ["HOST_OS"] = host_os

        from tools.azure import (
            az_login,
            check_deployment_status,
            create_resource_group,
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
            "check_permissions": lambda: check_permissions(),
            "create_resource_group": lambda: create_resource_group(
                args["name"], args["location"], host_os=host_os
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
                args["command"], host_os=host_os
            ),
        }

        fn = dispatch.get(name)
        if fn is None:
            return f"Unknown tool: {name}"

        try:
            result = fn()
            self._update_state(name, args, result)
            return result
        except Exception as e:
            return f"Error executing {name}: {str(e)}"

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

        elif tool_name == "check_deployment_status":
            # Result format: "Endpoint: https://...\nHealth: ..."
            for line in result.splitlines():
                if line.startswith("Endpoint: "):
                    self.installer_state["sao_endpoint"] = line[
                        len("Endpoint: ") :
                    ]
                    break

    def _load_system_prompt(self) -> str:
        """Load the system prompt from the bundled markdown file."""
        # In container: /app/system_prompt.md
        # Local dev: relative to project root
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
