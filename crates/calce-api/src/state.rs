use std::sync::Arc;

use calce_data::engine::AsyncCalcEngine;

#[derive(Clone)]
pub struct AppState {
    pub engine: Arc<AsyncCalcEngine>,
}
