use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use calce_data::auth::authz;
use calce_data::error::DataError;
use calce_data::queries::user_data::{User, UserDataRepo};
use serde::Deserialize;

use crate::auth::Auth;
use crate::error::ApiError;
use crate::state::AppState;

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/v1/users", get(list_users).post(create_user))
        .route(
            "/v1/users/{user_id}",
            get(get_user).put(update_user).delete(delete_user),
        )
}

fn repo(state: &AppState) -> Result<UserDataRepo, ApiError> {
    let pool = state
        .pool
        .as_ref()
        .ok_or_else(|| ApiError::BadRequest("CRUD requires Postgres backend".into()))?;
    Ok(UserDataRepo::new(pool.clone()))
}

async fn list_users(
    Auth(ctx): Auth,
    State(state): State<AppState>,
) -> Result<Json<Vec<User>>, ApiError> {
    authz::require_admin(&ctx)?;
    let users = repo(&state)?.find_all_users().await?;
    Ok(Json(users))
}

#[derive(Deserialize)]
struct CreateUserRequest {
    external_id: String,
    email: Option<String>,
    name: Option<String>,
}

async fn create_user(
    Auth(ctx): Auth,
    State(state): State<AppState>,
    Json(body): Json<CreateUserRequest>,
) -> Result<(StatusCode, Json<User>), ApiError> {
    authz::require_admin(&ctx)?;
    let user = repo(&state)?
        .create_user(
            &body.external_id,
            body.email.as_deref(),
            body.name.as_deref(),
        )
        .await?;
    Ok((StatusCode::CREATED, Json(user)))
}

async fn get_user(
    Auth(ctx): Auth,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<User>, ApiError> {
    authz::require_access(&ctx, &user_id)?;
    let user = repo(&state)?.get_user(&user_id).await?;
    Ok(Json(user))
}

#[derive(Deserialize)]
struct UpdateUserRequest {
    name: Option<String>,
    email: Option<String>,
}

async fn update_user(
    Auth(ctx): Auth,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    Json(body): Json<UpdateUserRequest>,
) -> Result<Json<User>, ApiError> {
    authz::require_access(&ctx, &user_id)?;
    // Email changes require admin privileges.
    if body.email.is_some() {
        authz::require_admin(&ctx)?;
    }
    let user = repo(&state)?
        .update_user(&user_id, body.name.as_deref(), body.email.as_deref())
        .await?;
    Ok(Json(user))
}

async fn delete_user(
    Auth(ctx): Auth,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    authz::require_admin(&ctx)?;
    let deleted = repo(&state)?.delete_user(&user_id).await?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::Data(DataError::NotFound(format!(
            "user '{user_id}'"
        ))))
    }
}
