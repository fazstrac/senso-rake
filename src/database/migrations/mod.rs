mod definition;
mod runner;

pub use definition::{Hash, MIGRATIONS, MigrationDefinition, MigrationError, MigrationRecord};
pub use runner::migrate_database;
