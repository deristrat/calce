use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use calce_data::auth::authz;
use calce_data::queries::user_data::{Organization, UserDataRepo};

use crate::auth::Auth;
use crate::error::ApiError;
use crate::state::AppState;

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/v1/organizations", get(list_organizations))
        .route("/v1/organizations/{org_id}", get(get_organization))
}

fn repo(state: &AppState) -> Result<UserDataRepo, ApiError> {
    Ok(UserDataRepo::new(state.require_pool()?.clone()))
}

async fn list_organizations(
    Auth(ctx): Auth,
    State(state): State<AppState>,
) -> Result<Json<Vec<Organization>>, ApiError> {
    authz::require_admin(&ctx)?;
    let orgs = repo(&state)?.find_all_organizations().await?;
    Ok(Json(orgs))
}

async fn get_organization(
    Auth(ctx): Auth,
    State(state): State<AppState>,
    Path(org_id): Path<String>,
) -> Result<Json<Organization>, ApiError> {
    authz::require_admin(&ctx)?;
    let org = repo(&state)?.get_organization(&org_id).await?;
    Ok(Json(org))
}
