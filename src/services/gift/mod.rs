//! Gift bot service: X webhook ingestion + on-chain consumer + reply.
//! Ported 1:1 from the gift-bot workspace; only the X ingestion transport
//! (filtered stream → webhook) changes. See
//! docs/superpowers/specs/2026-05-29-gift-bot-webhook-migration-design.md
pub mod config;
pub mod crc;
pub mod db;
pub mod db_retry;
pub mod domain;
pub mod executor;
pub mod ingest;
pub mod parser;
pub mod payload;
pub mod poller;
pub mod reconcile;
pub mod reply;
pub mod signature;
pub mod validator;
pub mod worker;
pub mod x_reply;
