//! Skeleton project for a Rust API application.
//!
//! This crate is intended to be used as a starting point for new API
//! applications. It mirrors the structure of the `skeleton-go-api` project:
//! a small CLI wrapping an HTTP server, layered configuration, structured
//! logging, a Postgres connection pool, and a sample service that fetches
//! photos from jsonplaceholder.typicode.com.

pub mod api;
pub mod client;
pub mod commands;
pub mod config;
pub mod db;
pub mod logger;
pub mod photos;
pub mod server;
