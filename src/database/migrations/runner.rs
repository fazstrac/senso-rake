use duckdb::types::{FromSql, FromSqlError, FromSqlResult, ValueRef};

use crate::database::{Hash, MIGRATIONS, MigrationDefinition, MigrationError, MigrationRecord};
use log::{error, info};

impl MigrationDefinition {
    fn apply(&self, conn: &duckdb::Connection) -> Result<(), MigrationError> {
        conn.execute(self.sql, []).map_err(|e| {
            error!("Error from DuckDb running migration {:?} {}", self.name, e);
            MigrationError::Database
        })?;

        conn.execute(
            r#"
            INSERT INTO
                senso_rake_schema_migrations
                (VERSION, NAME, CHECKSUM, APPLIED_AT)
            VALUES
                (?,?,?,CURRENT_TIMESTAMP)
            "#,
            (self.version, self.name, self.hash.to_string()),
        )
        .map_err(|e| {
            error!(
                "Error from DuckDb running migration `{:?}` {}",
                self.name, e
            );
            MigrationError::Database
        })?;

        Ok(())
    }
}

impl FromSql for Hash {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let value = value.as_str()?;

        Hash::try_from(value).map_err(|error| FromSqlError::Other(Box::new(error)))
    }
}

// This runs the migration inside a transaction
pub fn migrate_database(conn: &mut duckdb::Connection) -> Result<(), MigrationError> {
    let mut migration_definitions = MIGRATIONS.to_vec();
    let tx = conn.transaction().map_err(|e| {
        error!("Cannot start transaction: {:?}", e);
        MigrationError::Database
    })?;

    migrate_in_transaction(&tx, &mut migration_definitions)?;
    tx.commit().map_err(|e| {
        error!("Cannot commit transaction: {:?}", e);
        MigrationError::Database
    })?;

    Ok(())
}

fn table_exists(
    conn: &duckdb::Connection,
    table_name: &'static str,
) -> Result<bool, MigrationError> {
    conn.query_row(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM information_schema.tables
            WHERE table_schema = 'main'
              AND table_name = ?
        )
        "#,
        (table_name,),
        |row| row.get(0),
    )
    .map_err(|e| {
        error!("Error from DuckDb on checking migration table existence: \"{e}\"");
        MigrationError::Database
    })
}

fn get_applied_migrations(
    conn: &duckdb::Connection,
) -> Result<Vec<MigrationRecord>, MigrationError> {
    let mut statement = conn
        .prepare(
            r#"
        SELECT
            version,
            name,
            checksum
        FROM
            senso_rake_schema_migrations
        ORDER BY version ASC;
        "#,
        )
        .map_err(|e| {
            error!("Error from DuckDb on fetching migration data: \"{e}\"");
            MigrationError::Database
        })?;

    statement
        .query_map([], |row| {
            Ok(MigrationRecord {
                version: row.get(0)?,
                name: row.get(1)?,
                hash: row.get(2)?,
            })
        })
        .map_err(|e| {
            error!("Serialization error building MigrationRecord {e}");
            MigrationError::Serialization
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| {
            error!("Serialization error building Vec<MigrationRecord> {e}");
            MigrationError::Serialization
        })
}

fn migrate_in_transaction(
    conn: &duckdb::Connection,
    migration_definitions: &mut [MigrationDefinition],
) -> Result<(), MigrationError> {
    let applied_migrations = if table_exists(conn, "senso_rake_schema_migrations")? {
        get_applied_migrations(conn)?
    } else {
        Vec::new()
    };

    if (table_exists(conn, "mappings")? || table_exists(conn, "data_landing")?)
        && applied_migrations.is_empty()
    {
        error!("Unversioned database detected. Manual migration is needed.");
        return Err(MigrationError::UnversionedDatabase);
    }

    let applied_count = applied_migrations.len();

    migration_definitions.sort_by_key(|left| left.version);

    // This should be in `build.rs`. For now it can stay here.
    // 2026-08-19 Sami
    for pair in migration_definitions.windows(2) {
        if pair[1].version != pair[0].version + 1 {
            return Err(MigrationError::NonConsecutiveVersions);
        }
    }

    // Compare each applied version to its migration definition

    for (index, applied) in applied_migrations.iter().enumerate() {
        let migration_definition = migration_definitions.get(index);

        match migration_definition {
            Some(expected) => {
                // Requires that migration definitions are numbered consecutively
                if applied.version != expected.version {
                    error!("Database versions are not consecutive");
                    return Err(MigrationError::NonConsecutiveVersions);
                }

                if expected == applied {
                    Ok(())
                } else {
                    error!(
                        "Applied migration does not match definition {:?} vs {:?}",
                        applied, expected
                    );
                    Err(MigrationError::MigrationMismatch)
                }
            }
            None => {
                error!(
                    "Database has more applied migrations than this app has migration definitions"
                );
                Err(MigrationError::TooManyMigrations)
            }
        }?;
    }

    // Apply remaining migrations
    for migration in migration_definitions.iter().skip(applied_count) {
        info!("Running migration `{}`", migration.name);
        migration.apply(conn)?;
        info!("Migration `{}` succesful.", migration.name);
    }

    Ok(())
}
