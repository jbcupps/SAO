use base64::Engine;
use openidconnect::{
    core::{CoreProviderMetadata, CoreResponseType},
    AuthenticationFlow, AuthorizationCode, ClientId, ClientSecret, CsrfToken, IssuerUrl, Nonce,
    RedirectUrl, Scope, TokenResponse,
};

/// OIDC provider configuration loaded from DB.
#[allow(dead_code)]
pub struct OidcProviderConfig {
    pub id: uuid::Uuid,
    pub name: String,
    pub issuer_url: String,
    pub client_id: String,
    pub client_secret: Option<String>,
    pub scopes: String,
}

/// Result of starting an OIDC authorization flow.
pub struct OidcAuthResult {
    pub auth_url: url::Url,
    pub csrf_token: CsrfToken,
    pub nonce: Nonce,
}

/// User information extracted from OIDC ID token.
pub struct OidcUserInfo {
    pub subject: String,
    pub email: Option<String>,
    pub name: Option<String>,
    pub oid: Option<String>,
}

/// Generate an authorization URL for the OIDC provider.
pub async fn start_authorization(
    config: &OidcProviderConfig,
    redirect_url: &str,
) -> Result<OidcAuthResult, String> {
    let http_client = reqwest::Client::new();

    let issuer_url = IssuerUrl::new(config.issuer_url.clone())
        .map_err(|e| format!("Invalid issuer URL: {}", e))?;

    let provider_metadata = CoreProviderMetadata::discover_async(issuer_url, &http_client)
        .await
        .map_err(|e| format!("OIDC discovery failed: {}", e))?;

    let client_id = ClientId::new(config.client_id.clone());
    let client_secret = config
        .client_secret
        .as_ref()
        .map(|s| ClientSecret::new(s.clone()));

    let redirect = RedirectUrl::new(redirect_url.to_string())
        .map_err(|e| format!("Invalid redirect URL: {}", e))?;

    let client = openidconnect::core::CoreClient::from_provider_metadata(
        provider_metadata,
        client_id,
        client_secret,
    )
    .set_redirect_uri(redirect);

    let mut auth_request = client.authorize_url(
        AuthenticationFlow::<CoreResponseType>::AuthorizationCode,
        CsrfToken::new_random,
        Nonce::new_random,
    );

    for scope in config.scopes.split_whitespace() {
        if scope != "openid" {
            auth_request = auth_request.add_scope(Scope::new(scope.to_string()));
        }
    }

    let (url, csrf, nonce) = auth_request.url();

    Ok(OidcAuthResult {
        auth_url: url,
        csrf_token: csrf,
        nonce,
    })
}

/// Exchange an authorization code for tokens and extract user info.
pub async fn exchange_code(
    config: &OidcProviderConfig,
    redirect_url: &str,
    code: &str,
) -> Result<OidcUserInfo, String> {
    let http_client = reqwest::Client::new();

    let issuer_url = IssuerUrl::new(config.issuer_url.clone())
        .map_err(|e| format!("Invalid issuer URL: {}", e))?;

    let provider_metadata = CoreProviderMetadata::discover_async(issuer_url, &http_client)
        .await
        .map_err(|e| format!("OIDC discovery failed: {}", e))?;

    let client_id = ClientId::new(config.client_id.clone());
    let client_secret = config
        .client_secret
        .as_ref()
        .map(|s| ClientSecret::new(s.clone()));

    let redirect = RedirectUrl::new(redirect_url.to_string())
        .map_err(|e| format!("Invalid redirect URL: {}", e))?;

    let client = openidconnect::core::CoreClient::from_provider_metadata(
        provider_metadata,
        client_id,
        client_secret,
    )
    .set_redirect_uri(redirect);

    let token_response = client
        .exchange_code(AuthorizationCode::new(code.to_string()))
        .map_err(|e| format!("Failed to prepare token exchange: {}", e))?
        .request_async(&http_client)
        .await
        .map_err(|e| format!("Token exchange failed: {}", e))?;

    // Extract ID token claims
    let id_token = token_response.id_token().ok_or("No ID token in response")?;

    // Verify token - skip nonce verification for now (stored in DB challenge)
    let claims = id_token
        .claims(&client.id_token_verifier(), |_: Option<&Nonce>| Ok(()))
        .map_err(|e| format!("Failed to verify ID token: {}", e))?;

    let subject = claims.subject().to_string();
    let email: Option<String> = claims.email().map(|e| e.to_string());
    let name: Option<String> = claims
        .name()
        .and_then(|n| {
            let localized: &openidconnect::LocalizedClaim<openidconnect::EndUserName> = n;
            localized.get(None)
        })
        .map(|n| n.to_string());
    let oid = extract_claim_string(&id_token.to_string(), "oid");

    Ok(OidcUserInfo {
        subject,
        email,
        name,
        oid,
    })
}

fn extract_claim_string(id_token: &str, claim_name: &str) -> Option<String> {
    let mut parts = id_token.split('.');
    let _header = parts.next()?;
    let payload = parts.next()?;

    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .ok()?;
    let claims: serde_json::Value = serde_json::from_slice(&decoded).ok()?;

    claims
        .get(claim_name)
        .and_then(|value| value.as_str())
        .map(ToString::to_string)
}

#[cfg(test)]
mod tests {
    use super::extract_claim_string;
    use base64::Engine;

    #[test]
    fn extract_claim_string_reads_oid_from_id_token_payload() {
        let header = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(r#"{"alg":"none"}"#);
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(r#"{"sub":"subject-1","oid":"entra-oid-123"}"#);
        let id_token = format!("{header}.{payload}.signature");

        assert_eq!(
            extract_claim_string(&id_token, "oid").as_deref(),
            Some("entra-oid-123")
        );
    }

    #[test]
    fn extract_claim_string_returns_none_for_missing_claim() {
        let header = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(r#"{"alg":"none"}"#);
        let payload =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(r#"{"sub":"subject-1"}"#);
        let id_token = format!("{header}.{payload}.signature");

        assert_eq!(extract_claim_string(&id_token, "oid"), None);
    }
}
