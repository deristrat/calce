use chrono::NaiveDate;
use sqlx::PgPool;

use calce_core::domain::account::AccountId;
use calce_core::domain::currency::Currency;
use calce_core::domain::instrument::InstrumentId;
use calce_core::domain::price::Price;
use calce_core::domain::quantity::Quantity;
use calce_core::domain::trade::Trade;
use calce_core::domain::user::UserId;

use crate::error::DataResult;

pub struct UserDataRepo {
    pool: PgPool,
}

impl UserDataRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn get_trades(&self, user_id: &UserId) -> DataResult<Vec<Trade>> {
        let rows = sqlx::query_as::<_, TradeRow>(
            "SELECT user_id, account_id, instrument_id, quantity, price, currency, trade_date \
             FROM trades WHERE user_id = $1 ORDER BY trade_date, id",
        )
        .bind(user_id.as_str())
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(TradeRow::into_domain).collect())
    }

    pub async fn insert_user(&self, id: &UserId, email: Option<&str>) -> DataResult<()> {
        sqlx::query(
            "INSERT INTO users (id, email) VALUES ($1, $2) \
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(id.as_str())
        .bind(email)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn insert_account(
        &self,
        id: &AccountId,
        user_id: &UserId,
        currency: Currency,
        label: &str,
    ) -> DataResult<()> {
        sqlx::query(
            "INSERT INTO accounts (id, user_id, currency, label) VALUES ($1, $2, $3, $4) \
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(id.as_str())
        .bind(user_id.as_str())
        .bind(currency.as_str())
        .bind(label)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn insert_trade(&self, trade: &Trade) -> DataResult<()> {
        sqlx::query(
            "INSERT INTO trades (user_id, account_id, instrument_id, quantity, price, currency, trade_date) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(trade.user_id.as_str())
        .bind(trade.account_id.as_str())
        .bind(trade.instrument_id.as_str())
        .bind(trade.quantity.value())
        .bind(trade.price.value())
        .bind(trade.currency.as_str())
        .bind(trade.date)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

#[derive(sqlx::FromRow)]
struct TradeRow {
    user_id: String,
    account_id: String,
    instrument_id: String,
    quantity: f64,
    price: f64,
    currency: String,
    trade_date: NaiveDate,
}

impl TradeRow {
    fn into_domain(self) -> Trade {
        Trade {
            user_id: UserId::new(self.user_id),
            account_id: AccountId::new(self.account_id),
            instrument_id: InstrumentId::new(self.instrument_id),
            quantity: Quantity::new(self.quantity),
            price: Price::new(self.price),
            currency: Currency::new(&self.currency),
            date: self.trade_date,
        }
    }
}
