use webauthn_rs::prelude::*;

/// Create a Webauthn instance from environment configuration.
pub fn create_webauthn() -> Webauthn {
    let rp_id = std::env::var("SAO_RP_ID").unwrap_or_else(|_| "localhost".to_string());
    let rp_origin =
        std::env::var("SAO_RP_ORIGIN").unwrap_or_else(|_| "http://localhost:3100".to_string());

    let rp_origin_url = url::Url::parse(&rp_origin).expect("SAO_RP_ORIGIN must be a valid URL");

    let builder = WebauthnBuilder::new(&rp_id, &rp_origin_url)
        .expect("Failed to create WebauthnBuilder")
        .rp_name("SAO - Secure Agent Orchestrator");

    builder.build().expect("Failed to build Webauthn")
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
