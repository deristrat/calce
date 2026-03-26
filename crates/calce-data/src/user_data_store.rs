use std::collections::HashMap;

use serde::Serialize;

use crate::auth::SecurityContext;
use crate::error::{DataError, DataResult};
use crate::permissions;
use calce_core::domain::trade::Trade;
use calce_core::domain::user::UserId;

#[derive(Default)]
pub struct UserDataStore {
    trades: HashMap<UserId, Vec<Trade>>,
    users: Vec<UserSummary>,
}

#[derive(Clone, Serialize)]
pub struct UserSummary {
    pub id: String,
    pub email: Option<String>,
    pub organization_id: Option<String>,
    pub trade_count: i64,
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
    let mut seen = Vec::with_capacity(subjects.len());
    for s in subjects {
        if !seen.iter().any(|existing: &UserId| existing == s) {
            seen.push(s.clone());
        }
    }
    seen
}

impl UserDataStore {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_trade(&mut self, trade: Trade) {
        self.trades
            .entry(trade.user_id.clone())
            .or_default()
            .push(trade);
    }

    pub fn set_users(&mut self, users: Vec<UserSummary>) {
        self.users = users;
    }

    /// Derive the user list from trade keys (when no explicit user info is available).
    pub fn infer_users(&mut self) {
        self.users = self
            .trades
            .keys()
            .map(|id| UserSummary {
                id: id.as_str().to_owned(),
                email: None,
                organization_id: None,
                trade_count: 0,
            })
            .collect();
    }

    #[must_use]
    pub fn trades_for(&self, user_id: &UserId) -> Option<Vec<Trade>> {
        self.trades.get(user_id).cloned()
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
            all_trades.extend(trades);
        }

        Ok(all_trades)
    }

    /// List users visible to the caller.
    ///
    /// Admins see all users. Regular users see only themselves.
    pub fn list_users(&self, ctx: &SecurityContext) -> Vec<UserSummary> {
        if ctx.is_admin() {
            self.users.clone()
        } else {
            let id = ctx.user_id.as_str();
            self.users.iter().filter(|u| u.id == id).cloned().collect()
        }
    }

    pub fn user_count(&self) -> i64 {
        i64::try_from(self.users.len()).unwrap_or(0)
    }

    pub fn trade_count(&self) -> i64 {
        i64::try_from(self.trades.values().map(Vec::len).sum::<usize>()).unwrap_or(0)
    }

    pub fn organization_count(&self) -> i64 {
        let count = self
            .users
            .iter()
            .filter_map(|u| u.organization_id.as_deref())
            .collect::<std::collections::HashSet<_>>()
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
            user_id: UserId::new("alice"),
            account_id: AccountId::new(1),
            instrument_id: aapl,
            quantity: Quantity::new(100.0),
            price: Price::new(150.0),
            currency: usd,
            date: date(2025, 1, 10),
        });
        store.infer_users();
        store
    }

    fn admin_ctx() -> SecurityContext {
        SecurityContext {
            user_id: UserId::new("alice"),
            role: Role::Admin,
        }
    }

    fn user_ctx(user: &str) -> SecurityContext {
        SecurityContext {
            user_id: UserId::new(user),
            role: Role::User,
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
}
