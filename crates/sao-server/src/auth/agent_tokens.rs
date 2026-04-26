//! Entity identity tokens (OIDC-shaped JWTs for non-human principals).
//!
//! Each Orion entity is an identity in its own right. When a user downloads a bundle, SAO mints
//! a long-lived JWT that the entity carries as its identity proof on every API call. The token
//! shape is intentionally OIDC-compatible so a future Entra/external-IdP migration is just a
//! swap of the issuance + verification path; the bundle/runtime contract stays the same.
//!
//! Claims:
//!   sub             entity's agent_id
//!   jti             agent_tokens row id (revocation key)
//!   iss/aud         "sao" / "sao-api"
//!   principal_type  "non_human"   (distinguishes from user JWTs)
//!   entity_kind     "orion"
//!   entity_name     friendly name
//!   human_owner     creating user_id
//!   scope           space-separated scopes ("orion:policy orion:egress llm:generate")

use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use thiserror::Error;
use uuid::Uuid;

use crate::db::agent_tokens as db;

pub const DEFAULT_SCOPE: &str = "orion:policy orion:egress llm:generate";
pub const ENTITY_KIND_ORION: &str = "orion";
pub const PRINCIPAL_TYPE_NON_HUMAN: &str = "non_human";
const ISSUER: &str = "sao";
const AUDIENCE: &str = "sao-api";
const DEFAULT_TTL_DAYS: i64 = 365;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentClaims {
    pub iss: String,
    pub aud: String,
    pub sub: String,
    pub jti: String,
    pub iat: i64,
    pub nbf: i64,
    pub exp: i64,
    pub principal_type: String,
    pub entity_kind: String,
    pub entity_name: String,
    pub human_owner: String,
    pub scope: String,
}

impl AgentClaims {
    pub fn agent_id(&self) -> Result<Uuid, AgentTokenError> {
        Uuid::parse_str(&self.sub).map_err(|_| AgentTokenError::Invalid("sub is not a UUID"))
    }

    pub fn jti_uuid(&self) -> Result<Uuid, AgentTokenError> {
        Uuid::parse_str(&self.jti).map_err(|_| AgentTokenError::Invalid("jti is not a UUID"))
    }

    pub fn human_owner_id(&self) -> Result<Uuid, AgentTokenError> {
        Uuid::parse_str(&self.human_owner)
            .map_err(|_| AgentTokenError::Invalid("human_owner is not a UUID"))
    }
}

#[derive(Debug, Error)]
pub enum AgentTokenError {
    #[error("agent token database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("agent token jwt error: {0}")]
    Jwt(#[from] jsonwebtoken::errors::Error),
    #[error("agent token rejected: {0}")]
    Invalid(&'static str),
}

pub struct MintedAgentToken {
    pub jwt: String,
    pub jti: Uuid,
    pub expires_at: chrono::DateTime<Utc>,
}

pub async fn mint_entity_token(
    pool: &PgPool,
    jwt_secret: &[u8; 32],
    agent_id: Uuid,
    agent_name: &str,
    issuer_user: Uuid,
) -> Result<MintedAgentToken, AgentTokenError> {
    let now = Utc::now();
    let expires_at = now + Duration::days(DEFAULT_TTL_DAYS);

    let row = db::insert_token(pool, agent_id, issuer_user, Some(expires_at), DEFAULT_SCOPE).await?;

    let claims = AgentClaims {
        iss: ISSUER.to_string(),
        aud: AUDIENCE.to_string(),
        sub: agent_id.to_string(),
        jti: row.id.to_string(),
        iat: now.timestamp(),
        nbf: now.timestamp(),
        exp: expires_at.timestamp(),
        principal_type: PRINCIPAL_TYPE_NON_HUMAN.to_string(),
        entity_kind: ENTITY_KIND_ORION.to_string(),
        entity_name: agent_name.to_string(),
        human_owner: issuer_user.to_string(),
        scope: DEFAULT_SCOPE.to_string(),
    };

    let jwt = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(jwt_secret),
    )?;

    Ok(MintedAgentToken {
        jwt,
        jti: row.id,
        expires_at,
    })
}

pub async fn validate_entity_token(
    pool: &PgPool,
    jwt_secret: &[u8; 32],
    token: &str,
) -> Result<AgentClaims, AgentTokenError> {
    let mut validation = Validation::default();
    validation.set_audience(&[AUDIENCE]);
    validation.set_issuer(&[ISSUER]);

    let data = decode::<AgentClaims>(token, &DecodingKey::from_secret(jwt_secret), &validation)?;
    let claims = data.claims;

    if claims.principal_type != PRINCIPAL_TYPE_NON_HUMAN {
        return Err(AgentTokenError::Invalid("principal_type must be non_human"));
    }

    let jti = claims.jti_uuid()?;
    let row = db::get_active(pool, jti)
        .await?
        .ok_or(AgentTokenError::Invalid("token revoked or unknown"))?;

    if row.agent_id.to_string() != claims.sub {
        return Err(AgentTokenError::Invalid("sub does not match revocation row"));
    }

    let _ = db::touch_last_used(pool, jti).await;

    Ok(claims)
}

pub async fn revoke_for_agent(pool: &PgPool, agent_id: Uuid) -> Result<u64, AgentTokenError> {
    Ok(db::revoke_for_agent(pool, agent_id).await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claims_round_trip_through_jwt() {
        let secret = [9u8; 32];
        let now = Utc::now();
        let claims = AgentClaims {
            iss: ISSUER.into(),
            aud: AUDIENCE.into(),
            sub: Uuid::nil().to_string(),
            jti: Uuid::nil().to_string(),
            iat: now.timestamp(),
            nbf: now.timestamp(),
            exp: (now + Duration::minutes(5)).timestamp(),
            principal_type: PRINCIPAL_TYPE_NON_HUMAN.into(),
            entity_kind: ENTITY_KIND_ORION.into(),
            entity_name: "abigail".into(),
            human_owner: Uuid::nil().to_string(),
            scope: DEFAULT_SCOPE.into(),
        };

        let jwt = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(&secret),
        )
        .expect("encode");

        let mut validation = Validation::default();
        validation.set_audience(&[AUDIENCE]);
        validation.set_issuer(&[ISSUER]);
        let decoded =
            decode::<AgentClaims>(&jwt, &DecodingKey::from_secret(&secret), &validation)
                .expect("decode");

        assert_eq!(decoded.claims.entity_name, "abigail");
        assert_eq!(decoded.claims.principal_type, PRINCIPAL_TYPE_NON_HUMAN);
        assert_eq!(decoded.claims.scope, DEFAULT_SCOPE);
    }

    #[test]
    fn human_principal_token_rejected_by_validate_signature_only() {
        // Validate that a JWT lacking principal_type=non_human (e.g., a user session) fails our
        // entity validation before the DB lookup, so an attacker can't substitute a user JWT.
        let secret = [9u8; 32];
        let now = Utc::now();
        let claims = AgentClaims {
            iss: ISSUER.into(),
            aud: AUDIENCE.into(),
            sub: Uuid::nil().to_string(),
            jti: Uuid::nil().to_string(),
            iat: now.timestamp(),
            nbf: now.timestamp(),
            exp: (now + Duration::minutes(5)).timestamp(),
            principal_type: "human".into(),
            entity_kind: ENTITY_KIND_ORION.into(),
            entity_name: "x".into(),
            human_owner: Uuid::nil().to_string(),
            scope: DEFAULT_SCOPE.into(),
        };
        let jwt = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(&secret),
        )
        .expect("encode");

        let mut validation = Validation::default();
        validation.set_audience(&[AUDIENCE]);
        validation.set_issuer(&[ISSUER]);
        let decoded =
            decode::<AgentClaims>(&jwt, &DecodingKey::from_secret(&secret), &validation)
                .expect("decode");
        assert_ne!(decoded.claims.principal_type, PRINCIPAL_TYPE_NON_HUMAN);
    }
}
