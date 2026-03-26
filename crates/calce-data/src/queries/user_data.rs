use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use sqlx::PgPool;

use calce_core::domain::account::AccountId;
use calce_core::domain::currency::Currency;
use calce_core::domain::instrument::InstrumentId;
use calce_core::domain::price::Price;
use calce_core::domain::quantity::Quantity;
use calce_core::domain::trade::Trade;
use calce_core::domain::user::UserId;

use crate::error::{DataError, DataResult};

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct Organization {
    #[serde(rename = "id")]
    pub external_id: String,
    pub name: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct User {
    #[serde(rename = "id")]
    pub external_id: String,
    pub email: Option<String>,
    pub name: Option<String>,
    pub organization_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

pub struct UserDataRepo {
    pool: PgPool,
}

impl UserDataRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn get_trades(&self, user_id: &UserId) -> DataResult<Vec<Trade>> {
        let rows = sqlx::query_as::<_, TradeRow>(
            "SELECT u.external_id AS user_id, t.account_id, i.ticker AS instrument_id, \
                    t.quantity, t.price, t.currency, t.trade_date \
             FROM trades t \
             JOIN users u ON t.user_id = u.id \
             JOIN instruments i ON t.instrument_id = i.id \
             WHERE u.external_id = $1 ORDER BY t.trade_date, t.id",
        )
        .bind(user_id.as_str())
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(TradeRow::try_into_domain)
            .collect::<DataResult<Vec<_>>>()
    }

    pub async fn get_all_trades(&self) -> DataResult<Vec<Trade>> {
        let rows = sqlx::query_as::<_, TradeRow>(
            "SELECT u.external_id AS user_id, t.account_id, i.ticker AS instrument_id, \
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

    pub async fn upsert_user(&self, id: &UserId, email: Option<&str>) -> DataResult<()> {
        sqlx::query(
            "INSERT INTO users (external_id, email) VALUES ($1, $2) \
             ON CONFLICT (external_id) DO NOTHING",
        )
        .bind(id.as_str())
        .bind(email)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn insert_account(
        &self,
        user_id: &UserId,
        currency: Currency,
        label: &str,
    ) -> DataResult<AccountId> {
        let id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO accounts (user_id, currency, label) \
             VALUES ((SELECT id FROM users WHERE external_id = $1), $2, $3) \
             RETURNING id",
        )
        .bind(user_id.as_str())
        .bind(currency.as_str())
        .bind(label)
        .fetch_one(&self.pool)
        .await?;
        Ok(AccountId::new(id))
    }

    pub async fn list_users_with_trade_counts(&self) -> DataResult<Vec<UserRow>> {
        let rows = sqlx::query_as::<_, UserRow>(
            "SELECT u.external_id, u.email, o.external_id AS organization_id, \
                    COUNT(t.id)::BIGINT as trade_count \
             FROM users u \
             LEFT JOIN trades t ON u.id = t.user_id \
             LEFT JOIN organizations o ON u.organization_id = o.id \
             GROUP BY u.external_id, u.email, o.external_id \
             ORDER BY u.external_id",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn count_users_and_trades(&self) -> DataResult<(i64, i64)> {
        let row = sqlx::query_as::<_, (i64, i64)>(
            "SELECT (SELECT COUNT(*) FROM users), (SELECT COUNT(*) FROM trades)",
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn insert_trade(&self, trade: &Trade) -> DataResult<()> {
        sqlx::query(
            "INSERT INTO trades (user_id, account_id, instrument_id, quantity, price, currency, trade_date) \
             VALUES (\
                 (SELECT id FROM users WHERE external_id = $1), \
                 $2, \
                 (SELECT id FROM instruments WHERE ticker = $3), \
                 $4, $5, $6, $7)",
        )
        .bind(trade.user_id.as_str())
        .bind(trade.account_id.value())
        .bind(trade.instrument_id.as_str())
        .bind(trade.quantity.value())
        .bind(trade.price.value())
        .bind(trade.currency.as_str())
        .bind(trade.date)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    // ── CRUD operations ──────────────────────────────────────────────────

    pub async fn find_all_users(&self) -> DataResult<Vec<User>> {
        let users = sqlx::query_as::<_, User>(
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
        sqlx::query_as::<_, User>(
            "SELECT u.external_id, u.email, u.name, o.external_id AS organization_id, u.created_at \
             FROM users u \
             LEFT JOIN organizations o ON u.organization_id = o.id \
             WHERE u.external_id = $1",
        )
        .bind(external_id)
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
        sqlx::query_as::<_, User>(
            "INSERT INTO users (external_id, email, name) VALUES ($1, $2, $3) \
             RETURNING external_id, email, name, NULL::TEXT AS organization_id, created_at",
        )
        .bind(external_id)
        .bind(email)
        .bind(name)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DataError::from_constraint_violation(e, "user", external_id))
    }

    pub async fn update_user_name(
        &self,
        external_id: &str,
        name: Option<&str>,
    ) -> DataResult<User> {
        sqlx::query_as::<_, User>(
            "UPDATE users SET name = $2 WHERE external_id = $1 \
             RETURNING external_id, email, name, \
             (SELECT o.external_id FROM organizations o WHERE o.id = users.organization_id) AS organization_id, \
             created_at",
        )
        .bind(external_id)
        .bind(name)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| DataError::NotFound(format!("user '{external_id}'")))
    }

    // ── Organization queries ────────────────────────────────────────────

    pub async fn find_all_organizations(&self) -> DataResult<Vec<Organization>> {
        let orgs = sqlx::query_as::<_, Organization>(
            "SELECT external_id, name, created_at FROM organizations ORDER BY created_at",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(orgs)
    }

    pub async fn get_organization(&self, external_id: &str) -> DataResult<Organization> {
        sqlx::query_as::<_, Organization>(
            "SELECT external_id, name, created_at FROM organizations WHERE external_id = $1",
        )
        .bind(external_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| DataError::NotFound(format!("organization '{external_id}'")))
    }

    /// # Errors
    ///
    /// Returns `Conflict` if the user has dependent records (accounts, trades).
    pub async fn delete_user(&self, external_id: &str) -> DataResult<bool> {
        let result = sqlx::query("DELETE FROM users WHERE external_id = $1")
            .bind(external_id)
            .execute(&self.pool)
            .await
            .map_err(|e| DataError::from_constraint_violation(e, "user", external_id))?;
        Ok(result.rows_affected() > 0)
    }
}

#[derive(sqlx::FromRow)]
pub struct UserRow {
    pub external_id: String,
    pub email: Option<String>,
    pub organization_id: Option<String>,
    pub trade_count: Option<i64>,
}

#[derive(sqlx::FromRow)]
struct TradeRow {
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
