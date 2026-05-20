//! Concrete adapter implementations of ports.

pub mod aes_gcm;
pub mod chromiumoxide;
pub mod fs;
#[cfg(test)]
pub mod inmem;
pub mod postgres;
pub mod sqlite;
