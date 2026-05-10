//! sqlai-store：PostgreSQL + pgvector 持久化。

pub mod datasource;
pub mod error;
pub mod few_shot;
pub mod knowledge;
pub mod pool;
pub mod schema;
pub mod session;

pub use error::StoreError;
pub use pool::{connect, run_migrations, StoreConfig};
