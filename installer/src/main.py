#!/usr/bin/env python3
"""SAO Installer — Entry point."""

import argparse
import getpass
import os
import sys

from agent import InstallerAgent

BANNER = """
╔═══════════════════════════════════════════╗
║     SAO — Secure Agent Orchestrator       ║
║           Installation Agent              ║
╚═══════════════════════════════════════════╝
"""

PROVIDERS = {
    "1": ("claude", "Anthropic"),
    "2": ("openai", "OpenAI"),
    "3": ("xai", "xAI"),
    "4": ("google", "Google"),
    "5": ("custom", "Custom endpoint"),
}

PROVIDER_VISIBILITY_NOTICE = (
    "The installer runs locally, but the conversation and tool results are sent "
    "to Anthropic so the agent can operate. Local transcript files stay on this "
    "machine unless you choose to share them."
)


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    """Parse installer CLI arguments."""
    parser = argparse.ArgumentParser(description="SAO installer bootstrapper")
    parser.add_argument(
        "mode",
        nargs="?",
        choices=("cleanup", "uninstall"),
        help="Start directly in cleanup mode",
    )
    parser.add_argument(
        "--cleanup",
        action="store_true",
        help="Delete a prior SAO test resource group instead of starting an install",
    )
    parser.add_argument(
        "--resource-group",
        help="Azure resource group to target for cleanup",
    )
    return parser.parse_args(argv)


def collect_api_key() -> tuple[str, str]:
    """Collect LLM provider and API key. Returns (provider, api_key)."""
    print(BANNER)
    print("To guide you through this installation, I need an LLM API key.\n")
    print(PROVIDER_VISIBILITY_NOTICE)
    print()
    print("Provider options:")
    for k, (_, name) in PROVIDERS.items():
        suffix = "" if k == "1" else " — coming soon"
        print(f"  [{k}] {name}{suffix}")

    while True:
        choice = input("\nSelect provider (default: 1): ").strip() or "1"
        if choice not in PROVIDERS:
            print("Invalid selection.")
            continue
        if choice != "1":
            print(
                f"{PROVIDERS[choice][1]} support coming soon. "
                "Please use Claude for now."
            )
            continue
        break

    env_api_key = os.environ.get("ANTHROPIC_API_KEY", "").strip()
    if env_api_key:
        if not env_api_key.startswith("sk-ant-"):
            raise ValueError(
                "ANTHROPIC_API_KEY is set but does not start with 'sk-ant-'."
            )
        print("Using Anthropic API key from ANTHROPIC_API_KEY.")
        return "claude", env_api_key

    while True:
        try:
            api_key = getpass.getpass("Enter your Anthropic API key: ").strip()
        except (EOFError, KeyboardInterrupt):
            print()
            raise
        if not api_key.startswith("sk-ant-"):
            print(
                "That doesn't look like an Anthropic API key. "
                "It should start with 'sk-ant-'."
            )
            continue
        break

    return "claude", api_key


def main(argv: list[str] | None = None):
    args = parse_args(argv)
    cleanup_requested = args.cleanup or args.mode in {"cleanup", "uninstall"}

    if cleanup_requested:
        print(BANNER)
        resource_group = (args.resource_group or "").strip()
        if not resource_group:
            resource_group = input(
                "Enter the Azure resource group to clean up: "
            ).strip()
        agent = InstallerAgent(provider="cleanup", api_key=None)
        success = agent.run_cleanup_mode(resource_group)
        if not success:
            sys.exit(1)
        return

    try:
        provider, api_key = collect_api_key()
    except ValueError as exc:
        print(f"\n{exc}")
        sys.exit(1)

    agent = InstallerAgent(provider=provider, api_key=api_key)

    if not agent.verify_connection():
        print(
            "\nFailed to connect to Claude API. "
            "Check your key and try again."
        )
        sys.exit(1)

    print("\nConnecting to Claude... ✓\n")

    # Enter the agent conversation loop
    agent.run()


if __name__ == "__main__":
    main()
