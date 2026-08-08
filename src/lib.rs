//! `vocab` — capture the words you don't know as you read.
//!
//! The library exists so that tests can drive the whole application through its
//! one seam: construct [`App`] with its dependencies injected, send it key
//! events, and assert against what was rendered.

pub mod app;
pub mod deepseek;
pub mod dictionary;
pub mod domain;
pub mod notes;
pub mod search;
pub mod store;
pub mod ui;

pub use app::App;
pub use deepseek::DeepSeek;
pub use dictionary::Dictionary;
pub use notes::Notes;
pub use store::Store;
