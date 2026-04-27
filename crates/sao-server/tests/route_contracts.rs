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
    assert!(src.contains("/api/vault/secrets/:id"));
}

#[test]
fn oidc_routes_use_authorize_endpoint() {
    let src = include_str!("../src/routes/oidc.rs");
    assert!(src.contains("/api/auth/oidc/providers"));
    assert!(src.contains("/api/auth/oidc/:provider_id/authorize"));
    assert!(src.contains("/api/auth/oidc/callback"));
}

#[test]
fn admin_routes_use_oidc_provider_namespace() {
    let src = include_str!("../src/routes/admin.rs");
    assert!(src.contains("/api/admin/oidc/providers"));
    assert!(src.contains("/api/admin/oidc/providers/:id"));
}

#[test]
fn agents_routes_use_axum07_path_params_with_dual_method_delete() {
    let agents_src = include_str!("../src/routes/agents.rs");
    assert!(agents_src.contains("/api/agents/:id"));
    assert!(agents_src.contains("/api/agents/:id/delete"));
    assert!(agents_src.contains(".post(delete_agent_handler)"));
    assert!(agents_src.contains(".delete(delete_agent_handler)"));
    assert!(
        !agents_src.contains("/api/agents/{id}"),
        "axum 0.7 with matchit 0.7 does not parse `{{id}}`; use `:id` instead",
    );
}

#[test]
fn no_route_uses_brace_path_param_syntax_anywhere() {
    let route_files = [
        ("agents.rs", include_str!("../src/routes/agents.rs")),
        ("admin.rs", include_str!("../src/routes/admin.rs")),
        ("auth.rs", include_str!("../src/routes/auth.rs")),
        ("health.rs", include_str!("../src/routes/health.rs")),
        ("oidc.rs", include_str!("../src/routes/oidc.rs")),
        ("orion.rs", include_str!("../src/routes/orion.rs")),
        ("setup.rs", include_str!("../src/routes/setup.rs")),
        ("skills.rs", include_str!("../src/routes/skills.rs")),
        ("vault.rs", include_str!("../src/routes/vault.rs")),
    ];
    for (name, src) in route_files {
        for line in src.lines() {
            let trimmed = line.trim_start();
            if !trimmed.starts_with(".route(") {
                continue;
            }
            if let Some(start) = trimmed.find('"') {
                let rest = &trimmed[start + 1..];
                if let Some(end) = rest.find('"') {
                    let path = &rest[..end];
                    assert!(
                        !(path.contains('{') && path.contains('}')),
                        "{name}: route `{path}` uses brace-style placeholders; axum 0.7 with matchit 0.7 needs `:param` syntax",
                    );
                }
            }
        }
    }
}

#[test]
fn setup_routes_do_not_expose_legacy_initialize_endpoint() {
    let src = include_str!("../src/routes/setup.rs");
    assert!(src.contains("/api/setup/status"));
    assert!(!src.contains("/api/setup/initialize"));
}

#[test]
fn orion_routes_define_machine_policy_and_egress_contract() {
    let src = include_str!("../src/routes/orion.rs");
    assert!(src.contains("/api/orion/policy"));
    assert!(src.contains("/api/orion/egress"));
    assert!(src.contains("OrionBearerUser"));
    assert!(src.contains("OrionEgressRequest"));
}

#[test]
fn bundle_birth_and_llm_routes_define_entity_contracts() {
    let bundle_src = include_str!("../src/routes/bundle.rs");
    let orion_src = include_str!("../src/routes/orion.rs");
    let llm_src = include_str!("../src/routes/llm.rs");

    assert!(bundle_src.contains("/api/agents/:id/bundle"));
    assert!(bundle_src.contains("\"sao_base_url\""));
    assert!(bundle_src.contains("\"agent_token\""));
    assert!(bundle_src.contains("\"bus_transport\""));
    assert!(bundle_src.contains("\"nats_jetstream\""));
    assert!(bundle_src.contains("deployment.json"));
    assert!(bundle_src.contains("orionii.sao.deployment"));
    assert!(bundle_src.contains("\"downloaded_from\""));
    assert!(bundle_src.contains("Install-OrionII.cmd"));
    assert!(bundle_src.contains("Install-OrionII.ps1"));
    assert!(bundle_src.contains("public_base_url(&headers)"));
    assert!(bundle_src.contains("x-forwarded-host"));
    assert!(bundle_src.contains("GET /api/orion/birth"));
    assert!(bundle_src.contains("SAO does not participate in OrionII's internal bus"));

    assert!(orion_src.contains("/api/orion/birth"));
    assert!(orion_src.contains("OrionBirthResponse"));
    assert!(orion_src.contains("\"llm:generate\""));

    assert!(llm_src.contains("/api/llm/generate"));
    assert!(llm_src.contains("EntityCaller"));
    assert!(llm_src.contains("validate_entity_token"));
}

#[test]
fn security_keeps_orion_csrf_exception_scoped() {
    let src = include_str!("../src/security.rs");
    assert!(src.contains("is_orion_machine_request"));
    assert!(src.contains("\"/api/orion/egress\""));
    assert!(src.contains("\"/api/orion/birth\""));
    assert!(src.contains("\"/api/llm/generate\""));
    assert!(src.contains("requires_csrf(&method)"));
    assert!(src.contains("Bearer "));
}
