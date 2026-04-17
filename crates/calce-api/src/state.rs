use std::sync::Arc;

use calce_core::domain::currency::Currency;
use calce_core::domain::instrument::InstrumentId;
use calce_data::auth::AuthConfig;
use calce_data::auth::api_key::ApiKeyCache;
use calce_data::market_data_store::MarketDataStore;
use calce_data::user_data_store::UserDataStore;
use calce_datastructs::pubsub::PubSub;
use chrono::{DateTime, Utc};
use sqlx::PgPool;

use crate::db_simulator::DbSimulator;
use crate::rate_limit::KeyedRateLimiter;

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
    pub db_simulator: Option<Arc<DbSimulator>>,
    pub price_pubsub: Option<Arc<PricePubSub>>,
    pub fx_pubsub: Option<Arc<FxPubSub>>,
    pub entity_pubsub: Option<Arc<EntityPubSub>>,
    pub started_at: DateTime<Utc>,
}

impl AppState {
    pub(crate) fn require_pool(&self) -> Result<&PgPool, crate::error::ApiError> {
        self.pool
            .as_ref()
            .ok_or_else(|| crate::error::ApiError::BadRequest("database required".into()))
    }

    pub(crate) fn require_db_simulator(&self) -> Result<&Arc<DbSimulator>, crate::error::ApiError> {
        self.db_simulator.as_ref().ok_or_else(|| {
            crate::error::ApiError::BadRequest("database simulator not available".into())
        })
    }

    pub(crate) fn require_price_pubsub(&self) -> Result<&Arc<PricePubSub>, crate::error::ApiError> {
        self.price_pubsub
            .as_ref()
            .ok_or_else(|| crate::error::ApiError::BadRequest("price pubsub not available".into()))
    }

    pub(crate) fn require_fx_pubsub(&self) -> Result<&Arc<FxPubSub>, crate::error::ApiError> {
        self.fx_pubsub
            .as_ref()
            .ok_or_else(|| crate::error::ApiError::BadRequest("fx pubsub not available".into()))
    }

    pub(crate) fn require_entity_pubsub(
        &self,
    ) -> Result<&Arc<EntityPubSub>, crate::error::ApiError> {
        self.entity_pubsub
            .as_ref()
            .ok_or_else(|| crate::error::ApiError::BadRequest("entity pubsub not available".into()))
    }
}
