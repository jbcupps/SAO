use serde::{Deserialize, Serialize};

const ISSUE_CATALOG: &str =
    include_str!("../../../installer/shared/azure_bootstrap_issue_catalog.json");

#[derive(Debug, Clone, Serialize, Deserialize)]
struct IssueCatalog {
    issues: Vec<IssueDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct IssueDefinition {
    issue_type: String,
    #[serde(default)]
    diagnosis: String,
    #[serde(default)]
    guided_actions: Vec<String>,
    #[serde(default)]
    manual_commands: Vec<String>,
    #[serde(default)]
    safe_to_auto_apply: Vec<String>,
    #[serde(default)]
    r#match: IssueMatch,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct IssueMatch {
    #[serde(default)]
    resource_type_contains: Vec<String>,
    #[serde(default)]
    resource_name_contains: Vec<String>,
    #[serde(default)]
    all_of: Vec<String>,
    #[serde(default)]
    any_of: Vec<String>,
    #[serde(default)]
    not_any_of: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TroubleshootingRequest {
    pub resource_group: String,
    pub deployment_name: String,
    pub location: String,
    #[serde(default)]
    pub failed_resource_type: String,
    #[serde(default)]
    pub failed_resource_name: String,
    #[serde(default)]
    pub raw_error: String,
    #[serde(default)]
    pub issue_type_hint: String,
    #[serde(default)]
    pub image_reference: String,
    #[serde(default)]
    pub host_os: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TroubleshootingResponse {
    pub issue_type: String,
    pub diagnosis: String,
    pub evidence: Vec<String>,
    pub guided_actions: Vec<String>,
    pub manual_commands: Vec<String>,
    pub safe_to_auto_apply: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SkillError {
    InvalidInput(String),
    CatalogLoad(String),
}

fn load_catalog() -> Result<IssueCatalog, SkillError> {
    serde_json::from_str(ISSUE_CATALOG)
        .map_err(|err| SkillError::CatalogLoad(err.to_string()))
}

fn normalize(value: &str) -> String {
    value.trim().to_lowercase()
}

fn render_template(template: &str, request: &TroubleshootingRequest) -> String {
    template
        .replace("[[resource_group]]", request.resource_group.trim())
        .replace("[[deployment_name]]", request.deployment_name.trim())
        .replace("[[location]]", request.location.trim())
        .replace(
            "[[failed_resource_type]]",
            request.failed_resource_type.trim(),
        )
        .replace(
            "[[failed_resource_name]]",
            request.failed_resource_name.trim(),
        )
        .replace("[[image_reference]]", request.image_reference.trim())
}

fn matches_issue(issue: &IssueDefinition, request: &TroubleshootingRequest) -> bool {
    let searchable_text = format!(
        "{} {} {} {}",
        request.raw_error,
        request.failed_resource_type,
        request.failed_resource_name,
        request.image_reference
    )
    .to_lowercase();
    let resource_type = normalize(&request.failed_resource_type);
    let resource_name = normalize(&request.failed_resource_name);
    let hint = normalize(&request.issue_type_hint);

    if !hint.is_empty() && hint == normalize(&issue.issue_type) {
        return true;
    }

    if !issue.r#match.resource_type_contains.is_empty()
        && !issue
            .r#match
            .resource_type_contains
            .iter()
            .any(|token| {
                let token = token.to_lowercase();
                resource_type.contains(&token) || searchable_text.contains(&token)
            })
    {
        return false;
    }

    if !issue.r#match.resource_name_contains.is_empty()
        && !issue
            .r#match
            .resource_name_contains
            .iter()
            .any(|token| {
                let token = token.to_lowercase();
                resource_name.contains(&token) || searchable_text.contains(&token)
            })
    {
        return false;
    }

    if issue
        .r#match
        .all_of
        .iter()
        .any(|token| !searchable_text.contains(&token.to_lowercase()))
    {
        return false;
    }

    if !issue.r#match.any_of.is_empty()
        && !issue
            .r#match
            .any_of
            .iter()
            .any(|token| searchable_text.contains(&token.to_lowercase()))
    {
        return false;
    }

    if issue
        .r#match
        .not_any_of
        .iter()
        .any(|token| searchable_text.contains(&token.to_lowercase()))
    {
        return false;
    }

    true
}

pub fn execute(
    request: &TroubleshootingRequest,
) -> Result<TroubleshootingResponse, SkillError> {
    if request.resource_group.trim().is_empty() {
        return Err(SkillError::InvalidInput(
            "resource_group must not be empty".into(),
        ));
    }
    if request.deployment_name.trim().is_empty() {
        return Err(SkillError::InvalidInput(
            "deployment_name must not be empty".into(),
        ));
    }

    let catalog = load_catalog()?;
    let mut selected = None;
    let mut unknown = None;
    for issue in &catalog.issues {
        if issue.issue_type == "unknown" {
            unknown = Some(issue.clone());
            continue;
        }
        if matches_issue(issue, request) {
            selected = Some(issue.clone());
            break;
        }
    }
    let issue = selected.or(unknown).ok_or_else(|| {
        SkillError::CatalogLoad("catalog missing unknown fallback".into())
    })?;

    let evidence = request
        .raw_error
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(6)
        .map(str::to_string)
        .collect();

    Ok(TroubleshootingResponse {
        issue_type: issue.issue_type,
        diagnosis: issue.diagnosis,
        evidence,
        guided_actions: issue.guided_actions,
        manual_commands: issue
            .manual_commands
            .iter()
            .map(|command| render_template(command, request))
            .collect(),
        safe_to_auto_apply: issue.safe_to_auto_apply,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_request() -> TroubleshootingRequest {
        TroubleshootingRequest {
            resource_group: "sao-rg".into(),
            deployment_name: "sao-bootstrap".into(),
            location: "eastus2".into(),
            failed_resource_type: String::new(),
            failed_resource_name: String::new(),
            raw_error: String::new(),
            issue_type_hint: String::new(),
            image_reference: "ghcr.io/jbcupps/sao:latest".into(),
            host_os: "windows".into(),
        }
    }

    #[test]
    fn classifies_keyvault_soft_delete() {
        let mut request = base_request();
        request.failed_resource_type = "Microsoft.KeyVault/vaults".into();
        request.failed_resource_name = "sao-abc-kv".into();
        request.raw_error =
            "ConflictError: vault already exists in deleted state".into();

        let response = execute(&request).unwrap();
        assert_eq!(response.issue_type, "keyvault_soft_delete");
        assert!(response
            .guided_actions
            .contains(&"purge_deleted_key_vault".to_string()));
    }

    #[test]
    fn classifies_container_image_denied() {
        let mut request = base_request();
        request.deployment_name = "container-app".into();
        request.failed_resource_type = "Microsoft.App/containerApps".into();
        request.failed_resource_name = "sao-app".into();
        request.raw_error =
            "ContainerAppOperationError: DENIED: requested access to the resource is denied".into();

        let response = execute(&request).unwrap();
        assert_eq!(response.issue_type, "container_image_denied");
        assert!(response
            .manual_commands
            .iter()
            .any(|command| command.contains("registry set")));
    }

    #[test]
    fn falls_back_to_unknown() {
        let mut request = base_request();
        request.raw_error = "mystery failure".into();

        let response = execute(&request).unwrap();
        assert_eq!(response.issue_type, "unknown");
    }
}
