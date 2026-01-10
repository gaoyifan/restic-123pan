//! Restic REST API module.

pub mod handler;
pub mod types;

pub use handler::create_router;
