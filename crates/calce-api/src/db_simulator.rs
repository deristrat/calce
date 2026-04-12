//! Database-writing simulator for CDC pipeline testing.
//!
//! Unlike the [`crate::simulator`] which writes directly to the in-memory
//! cache, this simulator writes prices and FX rates to Postgres. The CDC
//! listener then picks up the WAL changes and propagates them back through
//! the cache → `PubSub` → SSE pipeline.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use calce_data::concurrent_market_data::ConcurrentMarketData;
use calce_data::queries::market_data::MarketDataRepo;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use tokio::sync::Notify;
use tokio::task::JoinHandle;

use crate::simulator::nudge;

// -- Config ------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbSimulatorConfig {
    /// Interval between ticks in milliseconds.
    #[serde(default = "default_tick_interval_ms")]
    pub tick_interval_ms: u64,
    /// Number of price writes per tick.
    #[serde(default = "default_prices_per_tick")]
    pub prices_per_tick: usize,
    /// Number of FX rate writes per tick.
    #[serde(default = "default_fx_per_tick")]
    pub fx_per_tick: usize,
}

fn default_tick_interval_ms() -> u64 {
    500
}
fn default_prices_per_tick() -> usize {
    5
}
fn default_fx_per_tick() -> usize {
    2
}

impl Default for DbSimulatorConfig {
    fn default() -> Self {
        Self {
            tick_interval_ms: default_tick_interval_ms(),
            prices_per_tick: default_prices_per_tick(),
            fx_per_tick: default_fx_per_tick(),
        }
    }
}

// -- Stats -------------------------------------------------------------------

#[derive(Default)]
struct AtomicStats {
    ticks: AtomicU64,
    price_writes: AtomicU64,
    fx_writes: AtomicU64,
    errors: AtomicU64,
}

#[derive(Serialize, Clone)]
pub struct DbSimulatorStats {
    pub running: bool,
    pub config: DbSimulatorConfig,
    pub ticks: u64,
    pub price_writes: u64,
    pub fx_writes: u64,
    pub errors: u64,
}

pub struct DbSimulator {
    market_data: Arc<ConcurrentMarketData>,
    repo: MarketDataRepo,
    running: AtomicBool,
    stop_notify: Notify,
    config: tokio::sync::Mutex<DbSimulatorConfig>,
    stats: AtomicStats,
    handle: tokio::sync::Mutex<Option<JoinHandle<()>>>,
}

impl DbSimulator {
    pub fn new(market_data: Arc<ConcurrentMarketData>, repo: MarketDataRepo) -> Self {
        Self {
            market_data,
            repo,
            running: AtomicBool::new(false),
            stop_notify: Notify::new(),
            config: tokio::sync::Mutex::new(DbSimulatorConfig::default()),
            stats: AtomicStats::default(),
            handle: tokio::sync::Mutex::new(None),
        }
    }

    pub async fn start(self: &Arc<Self>, cfg: DbSimulatorConfig) -> bool {
        if self.running.swap(true, Ordering::SeqCst) {
            return false;
        }

        *self.config.lock().await = cfg;

        self.stats.ticks.store(0, Ordering::Relaxed);
        self.stats.price_writes.store(0, Ordering::Relaxed);
        self.stats.fx_writes.store(0, Ordering::Relaxed);
        self.stats.errors.store(0, Ordering::Relaxed);

        let sim = Arc::clone(self);
        let handle = tokio::spawn(async move {
            sim.run_loop().await;
        });

        *self.handle.lock().await = Some(handle);
        true
    }

    pub async fn stop(self: &Arc<Self>) -> bool {
        if !self.running.swap(false, Ordering::SeqCst) {
            return false;
        }
        self.stop_notify.notify_one();
        if let Some(handle) = self.handle.lock().await.take() {
            let _ = handle.await;
        }
        true
    }

    pub async fn stats(&self) -> DbSimulatorStats {
        DbSimulatorStats {
            running: self.running.load(Ordering::Relaxed),
            config: self.config.lock().await.clone(),
            ticks: self.stats.ticks.load(Ordering::Relaxed),
            price_writes: self.stats.price_writes.load(Ordering::Relaxed),
            fx_writes: self.stats.fx_writes.load(Ordering::Relaxed),
            errors: self.stats.errors.load(Ordering::Relaxed),
        }
    }

    async fn run_loop(&self) {
        tracing::info!("DB simulator started");
        let cfg = self.config.lock().await.clone();
        let interval = Duration::from_millis(cfg.tick_interval_ms.max(10));

        loop {
            tokio::select! {
                () = tokio::time::sleep(interval) => {
                    self.tick(&cfg).await;
                }
                () = self.stop_notify.notified() => {
                    break;
                }
            }
        }

        tracing::info!("DB simulator stopped");
    }

    async fn tick(&self, cfg: &DbSimulatorConfig) {
        let today = chrono::Local::now().date_naive();

        // Collect write targets using rng in a non-async scope (ThreadRng is !Send).
        let (price_targets, fx_targets) = {
            let mut rng = rand::thread_rng();

            let instrument_ids = self.market_data.instrument_ids();
            let price_targets: Vec<_> = instrument_ids
                .choose_multiple(&mut rng, cfg.prices_per_tick.min(instrument_ids.len()))
                .filter_map(|id| {
                    self.market_data
                        .current_price(id)
                        .map(|p| (id.clone(), nudge(p).max(0.0)))
                })
                .collect();

            let fx_keys = self.market_data.fx_pair_keys();
            let fx_targets: Vec<_> = fx_keys
                .choose_multiple(&mut rng, cfg.fx_per_tick.min(fx_keys.len()))
                .filter_map(|&(from, to)| {
                    self.market_data
                        .current_fx_rate(from, to)
                        .map(|r| (from, to, nudge(r)))
                })
                .collect();

            (price_targets, fx_targets)
        };

        // -- Batch price writes ------------------------------------------------
        if !price_targets.is_empty() {
            let tickers: Vec<&str> = price_targets.iter().map(|(id, _)| id.as_str()).collect();
            let prices: Vec<f64> = price_targets.iter().map(|(_, p)| *p).collect();
            match self
                .repo
                .batch_upsert_prices(&tickers, today, &prices)
                .await
            {
                Ok(n) => {
                    self.stats.price_writes.fetch_add(n, Ordering::Relaxed);
                }
                Err(e) => {
                    tracing::warn!("DB simulator batch price write failed: {e}");
                    self.stats.errors.fetch_add(1, Ordering::Relaxed);
                }
            }
        }

        // -- Batch FX rate writes ----------------------------------------------
        if !fx_targets.is_empty() {
            let from_ccys: Vec<&str> = fx_targets.iter().map(|(f, _, _)| f.as_str()).collect();
            let to_ccys: Vec<&str> = fx_targets.iter().map(|(_, t, _)| t.as_str()).collect();
            let rates: Vec<f64> = fx_targets.iter().map(|(_, _, r)| *r).collect();
            match self
                .repo
                .batch_upsert_fx_rates(&from_ccys, &to_ccys, today, &rates)
                .await
            {
                Ok(n) => {
                    self.stats.fx_writes.fetch_add(n, Ordering::Relaxed);
                }
                Err(e) => {
                    tracing::warn!("DB simulator batch FX write failed: {e}");
                    self.stats.errors.fetch_add(1, Ordering::Relaxed);
                }
            }
        }

        self.stats.ticks.fetch_add(1, Ordering::Relaxed);
    }
}
