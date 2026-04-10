use std::sync::Arc;

use calce_core::domain::currency::Currency;
use calce_core::domain::instrument::InstrumentId;
use calce_data::auth::AuthConfig;
use calce_data::auth::api_key::ApiKeyCache;
use calce_data::market_data_store::MarketDataStore;
use calce_data::user_data_store::UserDataStore;
use calce_datastructs::pubsub::PubSub;
use sqlx::PgPool;

use crate::db_simulator::DbSimulator;
use crate::rate_limit::KeyedRateLimiter;
use crate::simulator::Simulator;

pub(crate) type PricePubSub = PubSub<InstrumentId>;
pub(crate) type FxPubSub = PubSub<(Currency, Currency)>;
pub(crate) type EntityPubSub = PubSub<String>;

#[derive(Clone)]
pub(crate) struct AppState {
    pub market_data: Arc<MarketDataStore>,
    pub user_data: Arc<UserDataStore>,
    pub pool: Option<PgPool>,
    pub auth_config: AuthConfig,
    pub api_key_cache: ApiKeyCache,
    pub auth_rate_limiter: Arc<KeyedRateLimiter>,
    pub simulator: Option<Arc<Simulator>>,
    pub db_simulator: Option<Arc<DbSimulator>>,
    pub price_pubsub: Option<Arc<PricePubSub>>,
    pub fx_pubsub: Option<Arc<FxPubSub>>,
    pub entity_pubsub: Option<Arc<EntityPubSub>>,
}

impl AppState {
    pub(crate) fn require_pool(&self) -> Result<&PgPool, crate::error::ApiError> {
        self.pool
            .as_ref()
            .ok_or_else(|| crate::error::ApiError::BadRequest("database required".into()))
    }
}
