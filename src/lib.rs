//! Financial calculation engine for portfolio tracking.
//!
//! Provides domain types, calculation functions, and service traits
//! for computing portfolio market values from trade history.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::panic)]
#![warn(clippy::pedantic)]

pub mod accounting;
pub mod auth;
pub mod calc;
pub mod context;
pub mod domain;
pub mod engine;
pub mod error;
pub mod reports;
pub mod services;
