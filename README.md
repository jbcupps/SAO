# SAO – Secure Agent Orchestrator

The enterprise control plane that installs itself via a governed AI agent conversation.

---

## Quick Start (Local Docker)

SAO keeps a Docker-first path for developers who want to run the stack locally.

```bash
git clone https://github.com/jbcupps/sao.git
cd sao
docker compose -f docker/docker-compose.yml up -d --build
curl http://localhost:3100/api/health
```

Expected health response:

```json
{
  "status": "ok",
  "service": "sao",
  "version": "0.0.1",
  "database": {
    "connected": true,
    "healthy": true
  }
}
```

Local notes:

- The server is exposed on `http://localhost:3100`.
- Docker Compose remains the recommended development workflow.
- On a fresh database, SAO enters installer mode rather than exposing a legacy setup wizard.

## Enterprise Vision & Installer

SAO is designed as an enterprise control plane for identity-bound agent operations, secure key custody, and governed orchestration. Its defining experience is the installer itself: instead of handing an operator a checklist, SAO boots into a managed conversation that provisions the platform with the same controls it will later enforce.

The container image for that experience is `ghcr.io/jbcupps/sao:installer`. It is intended to launch the first-run bootstrap inside a governed runtime, with the installer agent guiding the administrator through environment checks, identity confirmation, configuration capture, and validation of the platform state.

The bootstrap model is Entra-first:

- The first administrator authenticates through Microsoft Entra ID using OIDC.
- SAO records the authenticated Entra Object ID as the founding admin identity.
- No generated bootstrap passwords, seed users, or disposable credentials are created.
- Post-authentication configuration is completed through a conversational provisioning flow instead of a static wizard.

That provisioning flow is meant to be transparent and controlled:

- The installer explains each step before it acts.
- It can validate or help gather required tenant and application details.
- It can resume from partial progress instead of forcing a restart.
- It keeps the operator in a governed loop where identity, auditability, and orchestration begin together.

In enterprise demos, SAO should read as a control plane that installs itself through policy-aware dialogue: identity first, credentials never fabricated, and system state established through a traceable conversation.

## Architecture Highlights

### Birth documents

Every registered agent is grounded in signed origin material that establishes identity, constraints, and placement:

- `soul.md` for constitutional root
- `ethics.md` for ethical baseline
- `org-map.md` for organizational placement
- `personality.md` for adaptive expression

These birth documents anchor agent identity in signed artifacts rather than informal configuration alone.

### Bicameral Orion

SAO aligns with the Orion model by separating foundational identity and ethical structure from higher-level orchestration and adaptive behavior. The result is a bicameral pattern: constitutional roots remain stable while operational layers can deliberate, coordinate, and evolve under governance.

### Skill governance

Skills are treated as governed capability surfaces. SAO’s orchestration model assumes that specialized behaviors should be installed, approved, and routed with explicit boundaries rather than embedded as opaque logic. This supports enterprise review, safer reuse, and clearer operator control over what an agent is allowed to do.

### Ethical runtime verification

SAO is designed to verify more than configuration correctness. It aims to continuously check that orchestration behavior remains inside approved ethical and governance boundaries, connecting runtime action to declared constitutional and policy artifacts.

## Supporting Documents

- [SAO_Orion_Architecture_Analysis_v2.docx](documents/SAO_Orion_Architecture_Analysis_v2.docx)
- [Toward a Decentralized Trust Framework.pdf](documents/Toward%20a%20Decentralized%20Trust%20Framework.pdf)

## License

MIT
