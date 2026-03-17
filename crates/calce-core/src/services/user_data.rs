use std::collections::HashMap;

use crate::domain::trade::Trade;
use crate::domain::user::UserId;

/// In-memory user data store for testing.
#[derive(Default)]
pub struct InMemoryUserDataService {
    trades: HashMap<UserId, Vec<Trade>>,
}

impl InMemoryUserDataService {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn trades_for(&self, user_id: &UserId) -> Option<Vec<Trade>> {
        self.trades.get(user_id).cloned()
    }

    pub fn add_trade(&mut self, trade: Trade) {
        self.trades
            .entry(trade.user_id.clone())
            .or_default()
            .push(trade);
    }

    #[must_use]
    pub fn user_count(&self) -> usize {
        self.trades.len()
    }

    #[must_use]
    pub fn trade_count(&self) -> usize {
        self.trades.values().map(Vec::len).sum()
    }

    #[must_use]
    pub fn user_ids(&self) -> Vec<String> {
        self.trades
            .keys()
            .map(|id| id.as_str().to_owned())
            .collect()
    }
}
