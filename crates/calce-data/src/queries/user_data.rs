use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use sqlx::PgPool;

use calce_core::domain::account::AccountId;
use calce_core::domain::currency::Currency;
use calce_core::domain::instrument::InstrumentId;
use calce_core::domain::price::Price;
use calce_core::domain::quantity::Quantity;
use calce_core::domain::trade::{Trade, TradeId};
use calce_core::domain::user::UserId;

use crate::error::{DataError, DataResult};

#[derive(Debug, Serialize)]
pub struct Organization {
    #[serde(rename = "id")]
    pub external_id: String,
    pub name: Option<String>,
    pub created_at: DateTime<Utc>,
    pub user_count: i64,
}

#[derive(Debug, Serialize)]
pub struct User {
    #[serde(rename = "id")]
    pub external_id: String,
    pub email: Option<String>,
    pub name: Option<String>,
    pub organization_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct AccountSummary {
    pub id: i64,
    pub label: String,
    pub currency: String,
    pub trade_count: i64,
    pub position_count: i64,
    pub market_value: Option<f64>,
}

pub struct UserDataRepo {
    pool: PgPool,
}

impl UserDataRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn get_all_trades(&self) -> DataResult<Vec<Trade>> {
        let rows = sqlx::query_as!(
            TradeRow,
            "SELECT t.id, u.external_id AS user_id, t.account_id, i.ticker AS instrument_id, \
                    t.quantity, t.price, t.currency, t.trade_date \
             FROM trades t \
             JOIN users u ON t.user_id = u.id \
             JOIN instruments i ON t.instrument_id = i.id \
             ORDER BY u.external_id, t.trade_date, t.id",
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(TradeRow::try_into_domain)
            .collect::<DataResult<Vec<_>>>()
    }

    pub(crate) async fn list_users_with_trade_counts(&self) -> DataResult<Vec<UserRow>> {
        let rows = sqlx::query_as!(
            UserRow,
            "SELECT u.external_id, u.email, u.name, \
                    o.external_id AS organization_id, o.name AS organization_name, \
                    COUNT(DISTINCT t.id)::BIGINT AS trade_count, \
                    COUNT(DISTINCT a.id)::BIGINT AS account_count \
             FROM users u \
             LEFT JOIN trades t ON u.id = t.user_id \
             LEFT JOIN organizations o ON u.organization_id = o.id \
             LEFT JOIN accounts a ON u.id = a.user_id \
             GROUP BY u.external_id, u.email, u.name, o.external_id, o.name \
             ORDER BY u.external_id",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Lightweight lookup: just account (id → label) for a user. No aggregation.
    pub async fn get_account_names(&self, external_id: &str) -> DataResult<Vec<(i64, String)>> {
        let rows = sqlx::query!(
            "SELECT a.id, a.label \
             FROM accounts a \
             JOIN users u ON a.user_id = u.id \
             WHERE u.external_id = $1 \
             ORDER BY a.label",
            external_id,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|r| (r.id, r.label)).collect())
    }

    pub async fn get_user_accounts(&self, external_id: &str) -> DataResult<Vec<AccountSummary>> {
        let rows = sqlx::query_as!(
            AccountSummary,
            r#"WITH user_accounts AS (
                 SELECT a.id, a.label, a.currency
                 FROM accounts a
                 JOIN users u ON a.user_id = u.id
                 WHERE u.external_id = $1
             ),
             account_positions AS (
                 SELECT t.account_id, t.instrument_id,
                        SUM(t.quantity) AS net_quantity,
                        COUNT(t.id) AS trade_count
                 FROM trades t
                 WHERE t.account_id IN (SELECT id FROM user_accounts)
                 GROUP BY t.account_id, t.instrument_id
             ),
             needed_instruments AS (
                 SELECT DISTINCT instrument_id FROM account_positions
             ),
             latest_prices AS (
                 SELECT DISTINCT ON (p.instrument_id) p.instrument_id, p.price
                 FROM prices p
                 JOIN needed_instruments ni ON ni.instrument_id = p.instrument_id
                 ORDER BY p.instrument_id, p.price_date DESC
             )
             SELECT ua.id AS "id!", ua.label AS "label!", ua.currency AS "currency!",
                    COALESCE(SUM(ap.trade_count), 0)::BIGINT AS "trade_count!",
                    COUNT(ap.instrument_id)::BIGINT AS "position_count!",
                    SUM(ap.net_quantity * lp.price) AS market_value
             FROM user_accounts ua
             LEFT JOIN account_positions ap ON ap.account_id = ua.id
             LEFT JOIN latest_prices lp ON lp.instrument_id = ap.instrument_id
             GROUP BY ua.id, ua.label, ua.currency
             ORDER BY ua.label"#,
            external_id,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    // ── CRUD operations ──────────────────────────────────────────────────

    pub async fn find_all_users(&self) -> DataResult<Vec<User>> {
        let users = sqlx::query_as!(
            User,
            "SELECT u.external_id, u.email, u.name, o.external_id AS organization_id, u.created_at \
             FROM users u \
             LEFT JOIN organizations o ON u.organization_id = o.id \
             ORDER BY u.created_at",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(users)
    }

    pub async fn get_user(&self, external_id: &str) -> DataResult<User> {
        sqlx::query_as!(
            User,
            "SELECT u.external_id, u.email, u.name, o.external_id AS organization_id, u.created_at \
             FROM users u \
             LEFT JOIN organizations o ON u.organization_id = o.id \
             WHERE u.external_id = $1",
            external_id,
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| DataError::NotFound(format!("user '{external_id}'")))
    }

    pub async fn create_user(
        &self,
        external_id: &str,
        email: Option<&str>,
        name: Option<&str>,
    ) -> DataResult<User> {
        sqlx::query_as!(
            User,
            r#"INSERT INTO users (external_id, email, name) VALUES ($1, $2, $3)
             RETURNING external_id, email, name, NULL::TEXT AS "organization_id?", created_at"#,
            external_id,
            email,
            name,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DataError::from_constraint_violation(e, "user", external_id))
    }

    pub async fn update_user(
        &self,
        external_id: &str,
        name: Option<&str>,
        email: Option<&str>,
    ) -> DataResult<User> {
        sqlx::query_as!(
            User,
            r#"UPDATE users SET name = COALESCE($2, name), email = COALESCE($3, email)
             WHERE external_id = $1
             RETURNING external_id, email, name,
             (SELECT o.external_id FROM organizations o WHERE o.id = users.organization_id) AS "organization_id?",
             created_at"#,
            external_id,
            name,
            email,
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| DataError::NotFound(format!("user '{external_id}'")))
    }

    // ── Organization queries ────────────────────────────────────────────

    pub async fn find_all_organizations(&self) -> DataResult<Vec<Organization>> {
        let orgs = sqlx::query_as!(
            Organization,
            r#"SELECT o.external_id, o.name, o.created_at,
                    COUNT(u.id)::BIGINT AS "user_count!"
             FROM organizations o
             LEFT JOIN users u ON u.organization_id = o.id
             GROUP BY o.id
             ORDER BY o.created_at"#,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(orgs)
    }

    pub async fn get_organization(&self, external_id: &str) -> DataResult<Organization> {
        sqlx::query_as!(
            Organization,
            r#"SELECT o.external_id, o.name, o.created_at,
                    COUNT(u.id)::BIGINT AS "user_count!"
             FROM organizations o
             LEFT JOIN users u ON u.organization_id = o.id
             WHERE o.external_id = $1
             GROUP BY o.id"#,
            external_id,
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| DataError::NotFound(format!("organization '{external_id}'")))
    }

    /// # Errors
    ///
    /// Returns `Conflict` if the user has dependent records (accounts, trades).
    pub async fn delete_user(&self, external_id: &str) -> DataResult<bool> {
        let result = sqlx::query!("DELETE FROM users WHERE external_id = $1", external_id)
            .execute(&self.pool)
            .await
            .map_err(|e| DataError::from_constraint_violation(e, "user", external_id))?;
        Ok(result.rows_affected() > 0)
    }
}

pub(crate) struct UserRow {
    pub external_id: String,
    pub email: Option<String>,
    pub name: Option<String>,
    pub organization_id: Option<String>,
    pub organization_name: Option<String>,
    pub trade_count: Option<i64>,
    pub account_count: Option<i64>,
}

struct TradeRow {
    id: i64,
    user_id: String,
    account_id: i64,
    instrument_id: String,
    quantity: f64,
    price: f64,
    currency: String,
    trade_date: NaiveDate,
}

impl TradeRow {
    fn try_into_domain(self) -> DataResult<Trade> {
        let currency = Currency::try_new(&self.currency).map_err(|_| DataError::InvalidDbData {
            column: "currency".into(),
            value: self.currency.clone(),
            reason: "not a valid 3-letter uppercase currency code".into(),
        })?;
        Ok(Trade {
            id: Some(TradeId::new(self.id)),
            user_id: UserId::new(self.user_id),
            account_id: AccountId::new(self.account_id),
            instrument_id: InstrumentId::new(self.instrument_id),
            quantity: Quantity::new(self.quantity),
            price: Price::new(self.price),
            currency,
            date: self.trade_date,
        })
    }
}
