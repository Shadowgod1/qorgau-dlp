use std::sync::Arc;
use std::time::{Duration, Instant};

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::SaltString;
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::HeaderMap;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::Utc;
use dashmap::DashMap;
use hmac::{Hmac, Mac};
use qorgau_common::Role;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use sqlx::PgPool;
use subtle::ConstantTimeEq;

use crate::error::{AppError, AppResult};
use crate::state::AppState;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtClaims {
    pub sub: String,
    pub role: String,
    pub uid: i64,
    pub exp: i64,
    pub iat: i64,
}

#[derive(Debug, Clone)]
pub struct CurrentUser {
    pub id: i64,
    pub username: String,
    pub role: Role,
}

pub fn hash_password(password: &str) -> AppResult<String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    argon2
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| AppError::Internal(format!("Password hashing failed: {e}")))
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

pub fn create_jwt(secret: &str, user_id: i64, username: &str, role: Role, expire_min: i64) -> AppResult<String> {
    let now = Utc::now().timestamp();
    let claims = JwtClaims {
        sub: username.to_string(),
        role: role.as_str().to_string(),
        uid: user_id,
        iat: now,
        exp: now + expire_min * 60,
    };

    let header_b64 = URL_SAFE_NO_PAD.encode(br#"{"alg":"HS256","typ":"JWT"}"#);
    let payload_json =
        serde_json::to_vec(&claims).map_err(|e| AppError::Internal(e.to_string()))?;
    let payload_b64 = URL_SAFE_NO_PAD.encode(&payload_json);
    let signing_input = format!("{header_b64}.{payload_b64}");

    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|e| AppError::Internal(e.to_string()))?;
    mac.update(signing_input.as_bytes());
    let sig_b64 = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());

    Ok(format!("{signing_input}.{sig_b64}"))
}

pub fn verify_jwt(secret: &str, token: &str) -> AppResult<JwtClaims> {
    let mut parts = token.split('.');
    let (Some(header_b64), Some(payload_b64), Some(sig_b64), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(AppError::Unauthorized("Invalid token format".to_string()));
    };

    let header_bytes = URL_SAFE_NO_PAD
        .decode(header_b64)
        .map_err(|_| AppError::Unauthorized("Invalid token header".to_string()))?;
    let header_val: serde_json::Value = serde_json::from_slice(&header_bytes)
        .map_err(|_| AppError::Unauthorized("Invalid token header".to_string()))?;
    if header_val.get("alg").and_then(|v| v.as_str()) != Some("HS256") {
        return Err(AppError::Unauthorized("Unsupported token algorithm".to_string()));
    }

    let signing_input = format!("{header_b64}.{payload_b64}");
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|e| AppError::Internal(e.to_string()))?;
    mac.update(signing_input.as_bytes());
    let expected = mac.finalize().into_bytes();

    let provided = URL_SAFE_NO_PAD
        .decode(sig_b64)
        .map_err(|_| AppError::Unauthorized("Invalid token signature".to_string()))?;

    if provided.len() != expected.len() || !bool::from(provided.ct_eq(&expected)) {
        return Err(AppError::Unauthorized("Invalid token signature".to_string()));
    }

    let payload_bytes = URL_SAFE_NO_PAD
        .decode(payload_b64)
        .map_err(|_| AppError::Unauthorized("Invalid token payload".to_string()))?;
    let claims: JwtClaims = serde_json::from_slice(&payload_bytes)
        .map_err(|_| AppError::Unauthorized("Invalid token claims".to_string()))?;

    if Utc::now().timestamp() >= claims.exp {
        return Err(AppError::Unauthorized("Token expired".to_string()));
    }

    Ok(claims)
}

/// Validates a JWT token AND verifies operator is still active in PostgreSQL,
/// using the current DB role (so demotions/deactivations take effect immediately).
pub async fn authenticate_token(state: &AppState, token: &str) -> AppResult<CurrentUser> {
    let claims = verify_jwt(&state.config.secret_key, token)?;

    let row: Option<(i64, String, String, bool)> = sqlx::query_as(
        "SELECT id, username, role, is_active FROM operators WHERE id = $1 AND username = $2",
    )
    .bind(claims.uid)
    .bind(&claims.sub)
    .fetch_optional(&state.db)
    .await?;

    let Some((id, username, role_str, is_active)) = row else {
        return Err(AppError::Unauthorized("Operator not found".to_string()));
    };
    if !is_active {
        return Err(AppError::Unauthorized("Operator account is deactivated".to_string()));
    }
    let role = Role::parse_strict(&role_str)
        .ok_or_else(|| AppError::Unauthorized("Invalid operator role".to_string()))?;

    Ok(CurrentUser { id, username, role })
}

pub fn extract_bearer_token(headers: &HeaderMap) -> Option<&str> {
    let val = headers.get(axum::http::header::AUTHORIZATION)?.to_str().ok()?;
    val.strip_prefix("Bearer ").or_else(|| val.strip_prefix("bearer "))
}

pub fn verify_agent_token(expected: &str, headers: &HeaderMap) -> AppResult<()> {
    let provided = headers
        .get("x-agent-token")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let a = expected.as_bytes();
    let b = provided.as_bytes();
    if a.len() == b.len() && bool::from(a.ct_eq(b)) {
        Ok(())
    } else {
        Err(AppError::Unauthorized("Invalid agent token".to_string()))
    }
}

/// Simple sliding-window login brute-force protection (max 10 failed attempts / 5 min per key).
#[derive(Default)]
pub struct LoginRateLimiter {
    attempts: DashMap<String, Vec<Instant>>,
}

impl LoginRateLimiter {
    pub fn check(&self, key: &str) -> AppResult<()> {
        let cutoff = Instant::now() - Duration::from_secs(300);
        if let Some(mut entry) = self.attempts.get_mut(key) {
            entry.retain(|&t| t > cutoff);
            if entry.len() >= 10 {
                return Err(AppError::TooManyRequests(
                    "Too many failed login attempts. Try again in 5 minutes.".to_string(),
                ));
            }
        }
        Ok(())
    }

    pub fn record_failure(&self, key: &str) {
        let now = Instant::now();
        let cutoff = now - Duration::from_secs(300);
        let mut entry = self.attempts.entry(key.to_string()).or_default();
        entry.retain(|&t| t > cutoff);
        entry.push(now);
    }

    pub fn clear(&self, key: &str) {
        self.attempts.remove(key);
    }
}

// ── Axum Extractors ──────────────────────────────────────────────────────────

pub struct AuthUser(pub CurrentUser);
pub struct OfficerUser(pub CurrentUser);
pub struct AdminUser(pub CurrentUser);

impl FromRequestParts<Arc<AppState>> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        let token = extract_bearer_token(&parts.headers)
            .ok_or_else(|| AppError::Unauthorized("Missing Authorization header".to_string()))?;
        let user = authenticate_token(state, token).await?;
        Ok(AuthUser(user))
    }
}

impl FromRequestParts<Arc<AppState>> for OfficerUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        let AuthUser(user) = AuthUser::from_request_parts(parts, state).await?;
        if !user.role.is_officer_or_admin() {
            return Err(AppError::Forbidden(
                "Requires security_officer or admin role".to_string(),
            ));
        }
        Ok(OfficerUser(user))
    }
}

impl FromRequestParts<Arc<AppState>> for AdminUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        let AuthUser(user) = AuthUser::from_request_parts(parts, state).await?;
        if !user.role.is_admin() {
            return Err(AppError::Forbidden("Requires admin role".to_string()));
        }
        Ok(AdminUser(user))
    }
}

pub async fn seed_default_admin(db: &PgPool, username: &str, password: &str) -> AppResult<()> {
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM operators")
        .fetch_one(db)
        .await?;
    if count.0 == 0 {
        let hash = hash_password(password)?;
        sqlx::query(
            "INSERT INTO operators (username, password_hash, role, full_name, is_active) \
             VALUES ($1, $2, 'admin', 'System Administrator', TRUE) ON CONFLICT (username) DO NOTHING",
        )
        .bind(username)
        .bind(hash)
        .execute(db)
        .await?;
        tracing::info!(username = %username, "Seeded initial admin operator");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jwt_roundtrip_and_tamper_rejection() {
        let secret = "super-secret-key-for-unit-tests-12345";
        let tok = create_jwt(secret, 7, "officer1", Role::SecurityOfficer, 60).unwrap();
        let claims = verify_jwt(secret, &tok).unwrap();
        assert_eq!(claims.uid, 7);
        assert_eq!(claims.sub, "officer1");
        assert_eq!(claims.role, "security_officer");

        // Wrong secret fails
        assert!(verify_jwt("wrong-secret", &tok).is_err());
        // Tampered token fails
        let tampered = format!("{}x", tok);
        assert!(verify_jwt(secret, &tampered).is_err());
    }

    #[test]
    fn argon2_password_roundtrip() {
        let h = hash_password("StrongPass!2026").unwrap();
        assert!(verify_password("StrongPass!2026", &h));
        assert!(!verify_password("wrong", &h));
    }
}
