use std::sync::Arc;

use axum::extract::{Path, State};
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use qorgau_common::Role;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::audit::write_audit;
use crate::auth::{create_jwt, hash_password, verify_password, AdminUser, AuthUser};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/auth/login", post(login))
        .route("/auth/me", get(me))
        .route("/auth/users", get(list_operators).post(create_operator))
        .route(
            "/auth/users/{op_id}",
            patch(update_operator).delete(delete_operator),
        )
}

#[derive(Deserialize)]
struct LoginReq {
    username: String,
    password: String,
}

#[derive(Serialize)]
struct LoginResp {
    access_token: String,
    token_type: &'static str,
    username: String,
    role: String,
}

async fn login(
    State(state): State<Arc<AppState>>,
    Json(body): Json<LoginReq>,
) -> AppResult<Json<LoginResp>> {
    let uname = body.username.trim().to_string();
    let limiter_key = format!("user:{uname}");
    state.login_limiter.check(&limiter_key)?;

    let row: Option<(i64, String, String, String, bool)> = sqlx::query_as(
        "SELECT id, username, password_hash, role, is_active FROM operators WHERE username = $1",
    )
    .bind(&uname)
    .fetch_optional(&state.db)
    .await?;

    let Some((id, username, pw_hash, role_str, is_active)) = row else {
        state.login_limiter.record_failure(&limiter_key);
        write_audit(
            &state.db,
            &uname,
            "unknown",
            "login_failed",
            "auth",
            json!({ "reason": "user_not_found" }),
        )
        .await;
        return Err(AppError::Unauthorized(
            "Invalid username or password".to_string(),
        ));
    };

    if !is_active || !verify_password(&body.password, &pw_hash) {
        state.login_limiter.record_failure(&limiter_key);
        write_audit(
            &state.db,
            &username,
            &role_str,
            "login_failed",
            "auth",
            json!({ "reason": if !is_active { "inactive" } else { "bad_password" } }),
        )
        .await;
        return Err(AppError::Unauthorized(
            "Invalid username or password".to_string(),
        ));
    }

    let role = Role::parse_strict(&role_str).unwrap_or(Role::Viewer);
    state.login_limiter.clear(&limiter_key);

    let _ = sqlx::query("UPDATE operators SET last_login = NOW() WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await;

    let token = create_jwt(
        &state.config.secret_key,
        id,
        &username,
        role,
        state.config.jwt_expire_min,
    )?;

    write_audit(
        &state.db,
        &username,
        role.as_str(),
        "login_success",
        "auth",
        json!({}),
    )
    .await;

    Ok(Json(LoginResp {
        access_token: token,
        token_type: "bearer",
        username,
        role: role.as_str().to_string(),
    }))
}

#[derive(Serialize)]
struct MeResp {
    id: i64,
    username: String,
    role: String,
}

async fn me(AuthUser(user): AuthUser) -> Json<MeResp> {
    Json(MeResp {
        id: user.id,
        username: user.username,
        role: user.role.as_str().to_string(),
    })
}

#[derive(Serialize, sqlx::FromRow)]
struct OperatorRow {
    id: i64,
    username: String,
    role: String,
    full_name: Option<String>,
    is_active: bool,
    created_at: Option<DateTime<Utc>>,
    last_login: Option<DateTime<Utc>>,
}

async fn list_operators(
    AdminUser(_): AdminUser,
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<Vec<OperatorRow>>> {
    let rows: Vec<OperatorRow> = sqlx::query_as(
        "SELECT id, username, role, full_name, is_active, created_at, last_login \
         FROM operators ORDER BY id ASC",
    )
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
struct OperatorCreate {
    username: String,
    password: String,
    role: String,
    full_name: Option<String>,
}

async fn create_operator(
    AdminUser(admin): AdminUser,
    State(state): State<Arc<AppState>>,
    Json(body): Json<OperatorCreate>,
) -> AppResult<Json<OperatorRow>> {
    let uname = body.username.trim();
    if uname.len() < 3 || uname.len() > 64 {
        return Err(AppError::BadRequest(
            "Username must be 3..64 characters".to_string(),
        ));
    }
    if body.password.len() < 8 {
        return Err(AppError::BadRequest(
            "Password must be at least 8 characters".to_string(),
        ));
    }
    let role = Role::parse_strict(&body.role)
        .ok_or_else(|| AppError::BadRequest("Invalid role".to_string()))?;

    let hash = hash_password(&body.password)?;
    let row: OperatorRow = sqlx::query_as(
        "INSERT INTO operators (username, password_hash, role, full_name, is_active) \
         VALUES ($1, $2, $3, $4, TRUE) \
         RETURNING id, username, role, full_name, is_active, created_at, last_login",
    )
    .bind(uname)
    .bind(hash)
    .bind(role.as_str())
    .bind(&body.full_name)
    .fetch_one(&state.db)
    .await
    .map_err(|e| AppError::from_db_unique(e, "Operator username already exists"))?;

    write_audit(
        &state.db,
        &admin.username,
        admin.role.as_str(),
        "operator_created",
        &format!("operator:{}", row.id),
        json!({ "username": row.username, "role": row.role }),
    )
    .await;

    Ok(Json(row))
}

#[derive(Deserialize)]
struct OperatorUpdate {
    role: Option<String>,
    password: Option<String>,
    is_active: Option<bool>,
    full_name: Option<String>,
}

async fn update_operator(
    AdminUser(admin): AdminUser,
    State(state): State<Arc<AppState>>,
    Path(op_id): Path<i64>,
    Json(body): Json<OperatorUpdate>,
) -> AppResult<Json<OperatorRow>> {
    let existing: Option<(String, String, Option<String>, bool, String)> = sqlx::query_as(
        "SELECT username, role, full_name, is_active, password_hash FROM operators WHERE id = $1",
    )
    .bind(op_id)
    .fetch_optional(&state.db)
    .await?;

    let Some((username, cur_role, cur_full_name, cur_active, cur_hash)) = existing else {
        return Err(AppError::NotFound("Operator not found".to_string()));
    };

    let new_role = match body.role {
        Some(r) => Role::parse_strict(&r)
            .ok_or_else(|| AppError::BadRequest("Invalid role".to_string()))?
            .as_str()
            .to_string(),
        None => cur_role,
    };
    let new_active = body.is_active.unwrap_or(cur_active);
    let new_full_name = body.full_name.or(cur_full_name);

    // Prevent overwriting password with empty string (bug fix!)
    let new_hash = match body.password.as_deref() {
        Some(p) if !p.is_empty() => {
            if p.len() < 8 {
                return Err(AppError::BadRequest(
                    "Password must be at least 8 characters".to_string(),
                ));
            }
            hash_password(p)?
        }
        _ => cur_hash,
    };

    // Prevent self-lockout
    if op_id == admin.id && (!new_active || new_role != "admin") {
        return Err(AppError::BadRequest(
            "Cannot deactivate or demote yourself".to_string(),
        ));
    }

    let row: OperatorRow = sqlx::query_as(
        "UPDATE operators SET role = $1, full_name = $2, is_active = $3, password_hash = $4 \
         WHERE id = $5 \
         RETURNING id, username, role, full_name, is_active, created_at, last_login",
    )
    .bind(&new_role)
    .bind(&new_full_name)
    .bind(new_active)
    .bind(&new_hash)
    .bind(op_id)
    .fetch_one(&state.db)
    .await?;

    write_audit(
        &state.db,
        &admin.username,
        admin.role.as_str(),
        "operator_updated",
        &format!("operator:{op_id}"),
        json!({ "username": username, "role": new_role, "is_active": new_active }),
    )
    .await;

    Ok(Json(row))
}

async fn delete_operator(
    AdminUser(admin): AdminUser,
    State(state): State<Arc<AppState>>,
    Path(op_id): Path<i64>,
) -> AppResult<Json<serde_json::Value>> {
    if op_id == admin.id {
        return Err(AppError::BadRequest(
            "Cannot delete your own account".to_string(),
        ));
    }
    let res = sqlx::query("DELETE FROM operators WHERE id = $1")
        .bind(op_id)
        .execute(&state.db)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound("Operator not found".to_string()));
    }
    write_audit(
        &state.db,
        &admin.username,
        admin.role.as_str(),
        "operator_deleted",
        &format!("operator:{op_id}"),
        json!({}),
    )
    .await;
    Ok(Json(json!({ "status": "deleted" })))
}
