mod api_keys;
pub(crate) mod auth;
mod calc;
mod db_simulator;
mod events;
mod organizations;
mod simulator;
mod system;
mod users;

use axum::Router;

use crate::state::AppState;

pub(crate) fn calc_routes() -> Router<AppState> {
    calc::routes()
}

pub(crate) fn user_routes() -> Router<AppState> {
    users::routes()
}

pub(crate) fn organization_routes() -> Router<AppState> {
    organizations::routes()
}

pub(crate) fn auth_routes() -> Router<AppState> {
    auth::routes()
}

pub(crate) fn api_key_routes() -> Router<AppState> {
    api_keys::routes()
}

pub(crate) fn simulator_routes() -> Router<AppState> {
    simulator::routes()
}

pub(crate) fn db_simulator_routes() -> Router<AppState> {
    db_simulator::routes()
}

pub(crate) fn event_routes() -> Router<AppState> {
    events::routes()
}

pub(crate) fn system_routes() -> Router<AppState> {
    system::routes()
}
