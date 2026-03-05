use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

use crate::error::DataResult;

const DEFAULT_DATABASE_URL: &str = "postgres://calce:calce@localhost:5433/calce";

pub async fn create_pool(database_url: Option<&str>) -> DataResult<PgPool> {
    let url = database_url.unwrap_or(DEFAULT_DATABASE_URL);
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(url)
        .await?;
    Ok(pool)
}

pub async fn run_migrations(pool: &PgPool) -> DataResult<()> {
    sqlx::migrate!("./migrations").run(pool).await?;
    Ok(())
}
