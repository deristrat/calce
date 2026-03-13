use chrono::{NaiveDate, Utc};
use clap::Parser;

use calce_integrations::njorda::{self, cache};

fn today() -> NaiveDate {
    Utc::now().date_naive()
}

#[derive(Parser)]
#[command(
    name = "njorda-fetch",
    about = "Fetch market data from njorda legacy DB"
)]
struct Args {
    #[arg(long, default_value = "2023-01-01")]
    from: NaiveDate,

    #[arg(long, default_value_t = today())]
    to: NaiveDate,

    /// Force re-fetch even if cache is fresh.
    #[arg(long)]
    fresh: bool,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();

    let cache_path = cache::cache_path();

    // Check cache staleness
    if !args.fresh && !cache::is_stale(&cache_path) {
        println!("Cache is fresh, loading from {}", cache_path.display());
        let cached = cache::load_from_file(&cache_path)?;
        njorda::print_summary(&cached);
        return Ok(());
    }

    let password =
        std::env::var("NJORDA_DB_PASSWORD").map_err(|_| "NJORDA_DB_PASSWORD env var not set")?;

    println!("Connecting to njorda legacy DB...");
    let loader = njorda::NjordaLoader::connect(&password).await?;

    println!("Fetching market data from {} to {}...", args.from, args.to);
    let cached = loader.fetch(args.from, args.to).await?;

    cache::save_to_file(&cache_path, &cached)?;
    println!("Cache written to {}", cache_path.display());

    njorda::print_summary(&cached);

    // Verify we can build the service from cache
    let svc = njorda::build_service(&cached)?;
    drop(svc);
    println!("Service build from cache: OK");

    Ok(())
}
