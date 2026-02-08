# SAO - Claude Code Project Guide

## Project Overview
SAO (Secure Agent Orchestrator) is the multi-agent management layer. It handles agent identity creation, cryptographic verification, and coordination. It does NOT contain agent-specific logic (that's in abigail).

## Build & Test
```bash
cargo build                    # Build all crates
cargo test                     # Run all tests
cargo run --bin sao-server     # Start the orchestration server
cargo clippy                   # Lint
```

## Architecture Rules

### Separation of Concerns
- SAO manages agent identities and orchestration ONLY
- SAO does NOT run agent logic, LLM providers, or skills
- Agent-specific code belongs in the `abigail` repo
- Ethical evaluation logic belongs in `Ethical_AI_Reg`
- SAO bridges between agents and the ethical platform

### Crate Structure
- `sao-core`: Pure library crate with no server dependencies. Contains identity management, master key operations, and bridge types.
- `sao-server`: Binary crate with Axum server. Depends on sao-core. Contains routes, state, WebSocket handler.

### Security
- Master key never leaves the SAO data directory
- Agent public keys are verified against master key signatures
- All agent registration requires Ed25519 signature verification
- Never log secrets or key material
- Use the SSRF validation pattern from abigail for any URL inputs

### Integration Protocol
- Agents connect via REST API or WebSocket
- SAO verifies agent identity before accepting connections
- Ethical evaluations are forwarded to Ethical_AI_Reg via REST
- WebSocket broadcasts for real-time event distribution

## Related Repos
- `abigail` - Agent implementation
- `Ethical_AI_Reg` - Ethical alignment platform
- `Phoenix` - Coordination and project tracking
