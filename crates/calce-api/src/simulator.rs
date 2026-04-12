//! Price update simulator for development and testing.
//!
//! Runs a background task that periodically mutates random prices, FX rates,
//! and historical entries in the in-memory cache. This exercises the pub/sub
//! pipeline and lets the frontend display live-updating data without a real
//! market data feed.
//!
//! **Not for production use.**

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use calce_data::concurrent_market_data::ConcurrentMarketData;
use rand::Rng;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use tokio::sync::Notify;
use tokio::task::JoinHandle;

// -- Config ------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulatorConfig {
    /// Interval between ticks in milliseconds.
    #[serde(default = "default_tick_interval_ms")]
    pub tick_interval_ms: u64,
    /// Number of instrument current prices to update each tick.
    #[serde(default = "default_prices_per_tick")]
    pub prices_per_tick: usize,
    /// Number of FX pairs to update each tick.
    #[serde(default = "default_fx_per_tick")]
    pub fx_per_tick: usize,
    /// Number of random historical prices to update each tick.
    #[serde(default = "default_history_per_tick")]
    pub history_per_tick: usize,
}

fn default_tick_interval_ms() -> u64 {
    100
}
fn default_prices_per_tick() -> usize {
    50
}
fn default_fx_per_tick() -> usize {
    10
}
fn default_history_per_tick() -> usize {
    10
}

impl Default for SimulatorConfig {
    fn default() -> Self {
        Self {
            tick_interval_ms: default_tick_interval_ms(),
            prices_per_tick: default_prices_per_tick(),
            fx_per_tick: default_fx_per_tick(),
            history_per_tick: default_history_per_tick(),
        }
    }
}

// -- Stats -------------------------------------------------------------------

#[derive(Default)]
struct AtomicStats {
    fx_updates: AtomicU64,
    price_updates: AtomicU64,
    history_updates: AtomicU64,
    ticks: AtomicU64,
    errors: AtomicU64,
}

#[derive(Serialize, Clone)]
pub struct SimulatorStats {
    pub running: bool,
    pub config: SimulatorConfig,
    pub ticks: u64,
    pub fx_updates: u64,
    pub price_updates: u64,
    pub history_updates: u64,
    pub errors: u64,
}

// -- Simulator state ---------------------------------------------------------

pub struct Simulator {
    market_data: Arc<ConcurrentMarketData>,
    running: AtomicBool,
    stop_notify: Notify,
    config: tokio::sync::Mutex<SimulatorConfig>,
    stats: AtomicStats,
    handle: tokio::sync::Mutex<Option<JoinHandle<()>>>,
}

impl Simulator {
    pub fn new(market_data: Arc<ConcurrentMarketData>) -> Self {
        Self {
            market_data,
            running: AtomicBool::new(false),
            stop_notify: Notify::new(),
            config: tokio::sync::Mutex::new(SimulatorConfig::default()),
            stats: AtomicStats::default(),
            handle: tokio::sync::Mutex::new(None),
        }
    }

    /// Start the simulator with the given config. Returns false if already running.
    pub async fn start(self: &Arc<Self>, cfg: SimulatorConfig) -> bool {
        if self.running.swap(true, Ordering::SeqCst) {
            return false;
        }

        *self.config.lock().await = cfg;

        self.stats.fx_updates.store(0, Ordering::Relaxed);
        self.stats.price_updates.store(0, Ordering::Relaxed);
        self.stats.history_updates.store(0, Ordering::Relaxed);
        self.stats.ticks.store(0, Ordering::Relaxed);
        self.stats.errors.store(0, Ordering::Relaxed);

        let sim = Arc::clone(self);
        let handle = tokio::spawn(async move {
            sim.run_loop().await;
        });

        *self.handle.lock().await = Some(handle);
        true
    }

    /// Stop the simulator. Returns false if not running.
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

    pub async fn stats(&self) -> SimulatorStats {
        SimulatorStats {
            running: self.running.load(Ordering::Relaxed),
            config: self.config.lock().await.clone(),
            ticks: self.stats.ticks.load(Ordering::Relaxed),
            fx_updates: self.stats.fx_updates.load(Ordering::Relaxed),
            price_updates: self.stats.price_updates.load(Ordering::Relaxed),
            history_updates: self.stats.history_updates.load(Ordering::Relaxed),
            errors: self.stats.errors.load(Ordering::Relaxed),
        }
    }

    // -- Internal loop -------------------------------------------------------

    async fn run_loop(&self) {
        tracing::info!("Price simulator started");
        let cfg = self.config.lock().await.clone();
        let interval = Duration::from_millis(cfg.tick_interval_ms.max(10));

        loop {
            tokio::select! {
                () = tokio::time::sleep(interval) => {
                    self.tick(&cfg);
                }
                () = self.stop_notify.notified() => {
                    break;
                }
            }
        }

        tracing::info!("Price simulator stopped");
    }

    fn tick(&self, cfg: &SimulatorConfig) {
        let mut rng = rand::thread_rng();
        let md = &self.market_data;

        // -- FX rate updates -------------------------------------------------
        let fx_keys = md.fx_pair_keys();
        if !fx_keys.is_empty() {
            let sample: Vec<_> = fx_keys
                .choose_multiple(&mut rng, cfg.fx_per_tick.min(fx_keys.len()))
                .collect();
            for &(from, to) in &sample {
                if let Some(rate) = md.current_fx_rate(*from, *to) {
                    let new_rate = nudge(rate);
                    if md.set_current_fx_rate(*from, *to, new_rate).is_err() {
                        self.stats.errors.fetch_add(1, Ordering::Relaxed);
                    } else {
                        self.stats.fx_updates.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        }

        // -- Current price updates -------------------------------------------
        let instrument_ids = md.instrument_ids();
        if !instrument_ids.is_empty() {
            let sample: Vec<_> = instrument_ids
                .choose_multiple(&mut rng, cfg.prices_per_tick.min(instrument_ids.len()))
                .collect();
            for id in &sample {
                if let Some(price) = md.current_price(id) {
                    let new_price = nudge(price);
                    if md.set_current_price(id, new_price).is_err() {
                        self.stats.errors.fetch_add(1, Ordering::Relaxed);
                    } else {
                        self.stats.price_updates.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        }

        // -- Historical price updates ----------------------------------------
        if !instrument_ids.is_empty() {
            let sample: Vec<_> = instrument_ids
                .choose_multiple(&mut rng, cfg.history_per_tick.min(instrument_ids.len()))
                .collect();
            for id in &sample {
                if let Some(len) = md.price_history_len(id) {
                    if len == 0 {
                        continue;
                    }
                    let idx = rng.gen_range(0..len);
                    if let Some(val) = self.market_data_history_value(id, idx)
                        && !val.is_nan()
                    {
                        let new_val = nudge(val);
                        if md.update_price_at_index(id, idx, new_val).is_err() {
                            self.stats.errors.fetch_add(1, Ordering::Relaxed);
                        } else {
                            self.stats.history_updates.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }
            }
        }

        self.stats.ticks.fetch_add(1, Ordering::Relaxed);
    }

    fn market_data_history_value(
        &self,
        instrument: &calce_core::domain::instrument::InstrumentId,
        index: usize,
    ) -> Option<f64> {
        let range = self
            .market_data
            .price_history_range(instrument, index, index + 1)?;
        range.into_iter().next()
    }
}

/// Nudge a value: if the second decimal digit is odd, increase by 0.01;
/// if even, decrease by 0.01. This makes prices oscillate without drift.
pub(crate) fn nudge(value: f64) -> f64 {
    // Extract second decimal digit: floor(value * 100) mod 10
    #[allow(clippy::cast_possible_truncation)]
    let cents = (value * 100.0).floor() as i64;
    let second_decimal = (cents % 10).unsigned_abs();
    if second_decimal % 2 == 1 {
        value + 0.01
    } else {
        value - 0.01
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nudge_odd_second_decimal_increases() {
        // 10.53 → second decimal is 3 (odd) → +0.01
        let result = nudge(10.53);
        assert!((result - 10.54).abs() < 1e-10);
    }

    #[test]
    fn nudge_even_second_decimal_decreases() {
        // 10.54 → second decimal is 4 (even) → -0.01
        let result = nudge(10.54);
        assert!((result - 10.53).abs() < 1e-10);
    }

    #[test]
    fn nudge_zero_second_decimal_decreases() {
        // 10.50 → second decimal is 0 (even) → -0.01
        let result = nudge(10.50);
        assert!((result - 10.49).abs() < 1e-10);
    }

    #[test]
    fn nudge_oscillates() {
        let mut v = 100.42;
        let v0 = v;
        // Even → decrease
        v = nudge(v); // 100.41
        // Odd → increase
        v = nudge(v); // 100.42
        assert!((v - v0).abs() < 1e-10);
    }
}
