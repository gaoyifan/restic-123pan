//! 123pan API client module.

pub mod auth;
pub mod client;
pub mod entity;
pub mod types;

#[cfg(test)]
mod tests;

pub use client::Pan123Client;
pub use types::*;
