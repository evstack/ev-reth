//! EV Deployer — genesis alloc generator for ev-reth contracts.
//!
//! This crate provides both a CLI tool and a library for generating genesis
//! alloc entries from declarative TOML configurations.

pub mod config;
pub mod contracts;
pub mod deploy;
pub mod genesis;
pub mod output;
