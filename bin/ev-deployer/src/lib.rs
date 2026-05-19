//! EV Deployer — genesis alloc generator for ev-reth contracts.
//!
//! This crate provides both a CLI tool and a library for generating genesis
//! alloc entries from declarative TOML configurations.

pub mod config;
pub mod contracts;
/// CREATE2 deploy pipeline for live chain deployment.
pub mod deploy;
pub mod genesis;
/// Dynamic config template generation for the `init` command.
pub mod init;
pub mod output;
