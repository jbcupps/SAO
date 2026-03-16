use anyhow::{anyhow, Context};
use webauthn_rs::prelude::*;

/// Create a Webauthn instance from environment configuration.
pub fn create_webauthn() -> anyhow::Result<Webauthn> {
    let rp_id = std::env::var("SAO_RP_ID").unwrap_or_else(|_| "localhost".to_string());
    let rp_origin =
        std::env::var("SAO_RP_ORIGIN").unwrap_or_else(|_| "http://localhost:3100".to_string());

    create_webauthn_from_config(&rp_id, &rp_origin)
}

pub(crate) fn create_webauthn_from_config(
    rp_id: &str,
    rp_origin: &str,
) -> anyhow::Result<Webauthn> {
    let rp_origin_url = url::Url::parse(rp_origin)
        .with_context(|| format!("SAO_RP_ORIGIN must be a valid URL, got {rp_origin}"))?;
    let builder = WebauthnBuilder::new(rp_id, &rp_origin_url)
        .map_err(|err| anyhow!("Failed to create WebauthnBuilder: {err}"))?
        .rp_name("SAO - Secure Agent Orchestrator");

    builder
        .build()
        .map_err(|err| anyhow!("Failed to build Webauthn: {err}"))
}

/// Start a WebAuthn registration ceremony for a user.
pub fn start_registration(
    webauthn: &Webauthn,
    user_id: uuid::Uuid,
    username: &str,
    display_name: &str,
    existing_credentials: Vec<CredentialID>,
) -> Result<(CreationChallengeResponse, PasskeyRegistration), WebauthnError> {
    let exclude = existing_credentials;
    webauthn.start_passkey_registration(
        Uuid::from_bytes(*user_id.as_bytes()),
        username,
        display_name,
        Some(exclude),
    )
}

/// Complete a WebAuthn registration ceremony.
pub fn finish_registration(
    webauthn: &Webauthn,
    reg_response: &RegisterPublicKeyCredential,
    reg_state: &PasskeyRegistration,
) -> Result<Passkey, WebauthnError> {
    webauthn.finish_passkey_registration(reg_response, reg_state)
}

/// Start a WebAuthn authentication ceremony.
pub fn start_authentication(
    webauthn: &Webauthn,
    credentials: Vec<Passkey>,
) -> Result<(RequestChallengeResponse, PasskeyAuthentication), WebauthnError> {
    webauthn.start_passkey_authentication(&credentials)
}

/// Complete a WebAuthn authentication ceremony.
pub fn finish_authentication(
    webauthn: &Webauthn,
    auth_response: &PublicKeyCredential,
    auth_state: &PasskeyAuthentication,
) -> Result<AuthenticationResult, WebauthnError> {
    webauthn.finish_passkey_authentication(auth_response, auth_state)
}

#[cfg(test)]
mod tests {
    use super::create_webauthn_from_config;

    #[test]
    fn create_webauthn_from_config_returns_contextual_error_for_invalid_origin() {
        let error = create_webauthn_from_config("localhost", "not a url")
            .expect_err("expected invalid WebAuthn origin to fail");

        assert!(
            error
                .to_string()
                .contains("SAO_RP_ORIGIN must be a valid URL"),
            "unexpected error: {error:#}"
        );
    }
}
