#!/usr/bin/env python3
"""SAO Installer — Entry point."""

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


def collect_api_key() -> tuple[str, str]:
    """Collect LLM provider and API key. Returns (provider, api_key)."""
    print(BANNER)
    print("To guide you through this installation, I need an LLM API key.\n")
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

    while True:
        api_key = input("Enter your Anthropic API key: ").strip()
        if not api_key.startswith("sk-ant-"):
            print(
                "That doesn't look like an Anthropic API key. "
                "It should start with 'sk-ant-'."
            )
            continue
        break

    return "claude", api_key


def main():
    provider, api_key = collect_api_key()

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
