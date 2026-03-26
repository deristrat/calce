mod calc;
mod organizations;
mod users;

use axum::Router;

use crate::state::AppState;

pub use calc::explorer;

pub fn calc_routes() -> Router<AppState> {
    calc::routes()
}

pub fn user_routes() -> Router<AppState> {
    users::routes()
}

pub fn organization_routes() -> Router<AppState> {
    organizations::routes()
}
