# Skill: azure-bootstrap-troubleshooting

**Purpose**
Classify Azure bootstrap failures, surface the evidence that matters, and return guided recovery actions without improvising CLI syntax.

**When Ego should call this tool**
- After an Azure bootstrap deployment fails.
- When the operator asks what the logs say or how to recover.
- When a SAO workflow wants typed remediation options instead of raw Azure CLI output.

**Input contract**
- `resource_group`
- `deployment_name`
- `location`
- `failed_resource_type`
- `failed_resource_name`
- `raw_error`
- `issue_type_hint`
- `image_reference`
- `host_os`

**Success Pattern**
Return a `TroubleshootingResponse` with `issue_type`, `diagnosis`, `evidence`, `guided_actions`, `manual_commands`, and `safe_to_auto_apply`.

**Failure Recovery**
If the issue does not match a known signature, return `unknown` and include the baseline deployment-inspection commands.
