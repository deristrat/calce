use std::collections::{HashMap, HashSet};

use dashmap::DashMap;
use serde::Serialize;

use crate::auth::SecurityContext;
use crate::error::{DataError, DataResult};
use crate::permissions;
use calce_core::domain::account::AccountId;
use calce_core::domain::currency::Currency;
use calce_core::domain::instrument::InstrumentId;
use calce_core::domain::trade::Trade;
use calce_core::domain::user::UserId;

#[derive(Clone, Serialize)]
pub struct PositionSummary {
    pub instrument_id: InstrumentId,
    pub quantity: f64,
    pub currency: Currency,
    pub trade_count: i64,
}

pub struct UserDataStore {
    trades: HashMap<UserId, Vec<Trade>>,
    users: DashMap<UserId, UserSummary>,
}

#[derive(Clone, Debug, Serialize)]
pub struct UserSummary {
    pub id: String,
    pub email: Option<String>,
    pub name: Option<String>,
    pub organization_id: Option<String>,
    pub organization_name: Option<String>,
    pub trade_count: i64,
    pub account_count: i64,
}

fn check_user_access(security_ctx: &SecurityContext, user_id: &UserId) -> DataResult<()> {
    if !permissions::can_access_user_data(security_ctx, user_id) {
        return Err(DataError::Unauthorized {
            requester: security_ctx.user_id.clone(),
            target: user_id.clone(),
        });
    }
    Ok(())
}

fn dedup_subjects(subjects: &[UserId]) -> Vec<UserId> {
    let mut seen = HashSet::with_capacity(subjects.len());
    subjects
        .iter()
        .filter(|s| seen.insert(*s))
        .cloned()
        .collect()
}

/// Aggregate trades into position summaries, grouped by (instrument, currency).
fn aggregate_positions<'a>(trades: impl Iterator<Item = &'a Trade>) -> Vec<PositionSummary> {
    let mut positions: HashMap<(&str, Currency), (f64, i64)> = HashMap::new();
    for trade in trades {
        let key = (trade.instrument_id.as_str(), trade.currency);
        let entry = positions.entry(key).or_insert((0.0, 0));
        entry.0 += trade.quantity.value();
        entry.1 += 1;
    }

    let mut result: Vec<PositionSummary> = positions
        .into_iter()
        .map(
            |((instrument_id, currency), (quantity, trade_count))| PositionSummary {
                instrument_id: InstrumentId::new(instrument_id),
                quantity,
                currency,
                trade_count,
            },
        )
        .collect();
    result.sort_by(|a, b| a.instrument_id.as_str().cmp(b.instrument_id.as_str()));
    result
}

impl Default for UserDataStore {
    fn default() -> Self {
        Self::new()
    }
}

impl UserDataStore {
    #[must_use]
    pub fn new() -> Self {
        Self {
            trades: HashMap::new(),
            users: DashMap::new(),
        }
    }

    pub fn add_trade(&mut self, trade: Trade) {
        self.trades
            .entry(trade.user_id.clone())
            .or_default()
            .push(trade);
    }

    pub fn set_users(&self, users: Vec<UserSummary>) {
        self.users.clear();
        for u in users {
            self.users.insert(UserId::new(&u.id), u);
        }
    }

    /// Update name/email for a single user in the in-memory store.
    pub(crate) fn update_user_info(&self, user_id: &str, name: Option<&str>, email: Option<&str>) {
        let uid = UserId::new(user_id);
        if let Some(mut user) = self.users.get_mut(&uid) {
            if let Some(name) = name {
                user.name = Some(name.to_owned());
            }
            if let Some(email) = email {
                user.email = Some(email.to_owned());
            }
        }
    }

    #[must_use]
    pub fn trades_for(&self, user_id: &UserId) -> Option<&[Trade]> {
        self.trades.get(user_id).map(Vec::as_slice)
    }

    /// Load trades for the given subjects, enforcing access control.
    ///
    /// # Errors
    ///
    /// Returns `Unauthorized` if the security context lacks access to any subject.
    /// Returns `NoTradesFound` if a subject has no trades.
    pub fn load_trades(
        &self,
        ctx: &SecurityContext,
        subjects: &[UserId],
    ) -> DataResult<Vec<Trade>> {
        let subjects = dedup_subjects(subjects);
        let mut all_trades = Vec::new();

        for subject in &subjects {
            check_user_access(ctx, subject)?;
            let trades = self
                .trades_for(subject)
                .ok_or_else(|| DataError::NoTradesFound(subject.clone()))?;
            all_trades.extend_from_slice(trades);
        }

        Ok(all_trades)
    }

    /// List users visible to the caller.
    ///
    /// Uses centralized access-control rules: unrestricted admins see all users,
    /// regular users see only themselves. Org-scoped admins are filtered by
    /// default — route handlers must add org-membership filtering if needed.
    pub fn list_users(&self, ctx: &SecurityContext) -> Vec<UserSummary> {
        self.users
            .iter()
            .filter(|entry| permissions::can_access_user_data(ctx, entry.key()))
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// Look up a single user by ID, enforcing access control.
    ///
    /// # Errors
    ///
    /// Returns `Unauthorized` if the security context lacks access.
    pub fn get_user(
        &self,
        ctx: &SecurityContext,
        user_id: &UserId,
    ) -> DataResult<Option<UserSummary>> {
        check_user_access(ctx, user_id)?;
        Ok(self.users.get(user_id).map(|entry| entry.value().clone()))
    }

    pub fn user_count(&self) -> i64 {
        i64::try_from(self.users.len()).unwrap_or(0)
    }

    pub fn trade_count(&self) -> i64 {
        i64::try_from(self.trades.values().map(Vec::len).sum::<usize>()).unwrap_or(0)
    }

    pub fn positions_for_user(
        &self,
        ctx: &SecurityContext,
        user_id: &UserId,
    ) -> DataResult<Vec<PositionSummary>> {
        check_user_access(ctx, user_id)?;
        let trades = self.trades_for(user_id).unwrap_or_default();
        Ok(aggregate_positions(trades.iter()))
    }

    pub fn positions_for_account(
        &self,
        ctx: &SecurityContext,
        user_id: &UserId,
        account_id: AccountId,
    ) -> DataResult<Vec<PositionSummary>> {
        check_user_access(ctx, user_id)?;
        let trades = self.trades_for(user_id).unwrap_or_default();
        Ok(aggregate_positions(
            trades.iter().filter(|t| t.account_id == account_id),
        ))
    }

    pub fn organization_count(&self) -> i64 {
        let count = self
            .users
            .iter()
            .filter_map(|entry| entry.value().organization_id.clone())
            .collect::<HashSet<_>>()
            .len();
        i64::try_from(count).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::Role;
    use calce_core::domain::account::AccountId;
    use calce_core::domain::currency::Currency;
    use calce_core::domain::price::Price;
    use calce_core::domain::quantity::Quantity;
    use calce_core::domain::trade::Trade;

    fn date(y: i32, m: u32, d: u32) -> chrono::NaiveDate {
        chrono::NaiveDate::from_ymd_opt(y, m, d).expect("valid test date")
    }

    fn test_store() -> UserDataStore {
        let usd = Currency::new("USD");
        let aapl = calce_core::domain::instrument::InstrumentId::new("AAPL");

        let mut store = UserDataStore::new();
        store.add_trade(Trade {
            id: None,
            user_id: UserId::new("alice"),
            account_id: AccountId::new(1),
            instrument_id: aapl,
            quantity: Quantity::new(100.0),
            price: Price::new(150.0),
            currency: usd,
            date: date(2025, 1, 10),
        });
        store.set_users(vec![UserSummary {
            id: "alice".to_owned(),
            email: None,
            name: None,
            organization_id: None,
            organization_name: None,
            trade_count: 1,
            account_count: 1,
        }]);
        store
    }

    fn admin_ctx() -> SecurityContext {
        SecurityContext {
            user_id: UserId::new("alice"),
            role: Role::Admin,
            org_id: None,
        }
    }

    fn user_ctx(user: &str) -> SecurityContext {
        SecurityContext {
            user_id: UserId::new(user),
            role: Role::User,
            org_id: None,
        }
    }

    #[test]
    fn load_trades_enforces_access_check() {
        let store = test_store();
        let err = store
            .load_trades(&user_ctx("bob"), &[UserId::new("alice")])
            .unwrap_err();
        assert!(matches!(err, DataError::Unauthorized { .. }));
    }

    #[test]
    fn load_trades_allows_self_access() {
        let store = test_store();
        let trades = store
            .load_trades(&user_ctx("alice"), &[UserId::new("alice")])
            .unwrap();
        assert_eq!(trades.len(), 1);
    }

    #[test]
    fn load_trades_allows_admin_access() {
        let store = test_store();
        let trades = store
            .load_trades(&admin_ctx(), &[UserId::new("alice")])
            .unwrap();
        assert_eq!(trades.len(), 1);
    }

    #[test]
    fn duplicate_subjects_are_deduplicated() {
        let store = test_store();
        let trades = store
            .load_trades(&admin_ctx(), &[UserId::new("alice"), UserId::new("alice")])
            .unwrap();
        assert_eq!(trades.len(), 1);
    }

    #[test]
    fn list_users_admin_sees_all() {
        let store = test_store();
        let users = store.list_users(&admin_ctx());
        assert_eq!(users.len(), 1);
        assert_eq!(users[0].id, "alice");
    }

    #[test]
    fn list_users_user_sees_only_self() {
        let store = test_store();
        let users = store.list_users(&user_ctx("alice"));
        assert_eq!(users.len(), 1);
        assert_eq!(users[0].id, "alice");

        let users = store.list_users(&user_ctx("bob"));
        assert!(users.is_empty());
    }

    #[test]
    fn get_user_denies_unauthorized() {
        let store = test_store();
        let err = store
            .get_user(&user_ctx("bob"), &UserId::new("alice"))
            .unwrap_err();
        assert!(matches!(err, DataError::Unauthorized { .. }));
    }

    #[test]
    fn get_user_allows_self() {
        let store = test_store();
        let user = store
            .get_user(&user_ctx("alice"), &UserId::new("alice"))
            .unwrap();
        assert_eq!(user.unwrap().id, "alice");
    }

    #[test]
    fn positions_for_user_aggregates_correctly() {
        let store = test_store();
        let positions = store
            .positions_for_user(&admin_ctx(), &UserId::new("alice"))
            .unwrap();
        assert_eq!(positions.len(), 1);
        assert_eq!(positions[0].instrument_id.as_str(), "AAPL");
        assert!((positions[0].quantity - 100.0).abs() < f64::EPSILON);
        assert_eq!(positions[0].currency.as_str(), "USD");
        assert_eq!(positions[0].trade_count, 1);
    }
}
