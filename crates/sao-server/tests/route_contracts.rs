#[test]
fn auth_routes_use_start_finish_paths() {
    let src = include_str!("../src/routes/auth.rs");
    assert!(src.contains("/api/auth/webauthn/register/start"));
    assert!(src.contains("/api/auth/webauthn/register/finish"));
    assert!(src.contains("/api/auth/webauthn/login/start"));
    assert!(src.contains("/api/auth/webauthn/login/finish"));
    assert!(src.contains("/api/auth/refresh"));
    assert!(src.contains("/api/auth/logout"));
}

#[test]
fn vault_routes_use_vault_secrets_namespace() {
    let src = include_str!("../src/routes/vault.rs");
    assert!(src.contains("/api/vault/status"));
    assert!(src.contains("/api/vault/unseal"));
    assert!(src.contains("/api/vault/seal"));
    assert!(src.contains("/api/vault/secrets"));
    assert!(src.contains("/api/vault/secrets/{id}"));
}

#[test]
fn oidc_routes_use_authorize_endpoint() {
    let src = include_str!("../src/routes/oidc.rs");
    assert!(src.contains("/api/auth/oidc/providers"));
    assert!(src.contains("/api/auth/oidc/{provider_id}/authorize"));
    assert!(src.contains("/api/auth/oidc/callback"));
}

#[test]
fn admin_routes_use_oidc_provider_namespace() {
    let src = include_str!("../src/routes/admin.rs");
    assert!(src.contains("/api/admin/oidc/providers"));
    assert!(src.contains("/api/admin/oidc/providers/{id}"));
}

#[test]
fn routes_use_brace_path_params_for_agents_without_public_ws_route() {
    let agents_src = include_str!("../src/routes/agents.rs");
    assert!(agents_src.contains("/api/agents/{id}"));
    assert!(!agents_src.contains("/api/agents/:id"));
}

#[test]
fn setup_routes_do_not_expose_legacy_initialize_endpoint() {
    let src = include_str!("../src/routes/setup.rs");
    assert!(src.contains("/api/setup/status"));
    assert!(!src.contains("/api/setup/initialize"));
}
