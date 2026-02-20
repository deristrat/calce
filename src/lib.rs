//! Financial calculation engine for portfolio tracking.
//!
//! Provides domain types, calculation functions, and service traits
//! for computing portfolio market values from trade history.

#![forbid(missing_docs)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::panic)]
#![warn(clippy::pedantic)]

/// Authentication and authorization.
pub mod auth;
/// Calculation functions and orchestration engine.
pub mod calc;
/// Immutable calculation context (base currency, as-of date).
pub mod context;
/// Core domain types: currencies, money, trades, positions, etc.
pub mod domain;
/// Error types and result alias.
pub mod error;
/// Service traits and in-memory implementations.
pub mod services;
