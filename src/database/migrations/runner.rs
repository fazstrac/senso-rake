use duckdb::Connection;
use duckdb::types::{FromSql, FromSqlError, FromSqlResult, ValueRef};

use crate::database::{Hash, MIGRATIONS, MigrationDefinition, MigrationError, MigrationRecord};
use log::{error, info};

impl MigrationDefinition {
    fn apply(&self, conn: &Connection) -> Result<(), MigrationError> {
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
pub fn migrate_database(conn: &mut Connection) -> Result<(), MigrationError> {
    let mut migration_definitions = MIGRATIONS.to_vec();

    migrate_database_with(conn, &mut migration_definitions)
}

// Private helper to simplify testing with mock data
fn migrate_database_with(
    conn: &mut Connection,
    migration_definitions: &mut [MigrationDefinition],
) -> Result<(), MigrationError> {
    let tx = conn.transaction().map_err(|e| {
        error!("Cannot start transaction: {:?}", e);
        MigrationError::Database
    })?;

    migrate_in_transaction(&tx, migration_definitions)?;
    tx.commit().map_err(|e| {
        error!("Cannot commit transaction: {:?}", e);
        MigrationError::Database
    })?;

    Ok(())
}

fn table_exists(conn: &Connection, table_name: &'static str) -> Result<bool, MigrationError> {
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

fn get_applied_migrations(conn: &Connection) -> Result<Vec<MigrationRecord>, MigrationError> {
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
    conn: &Connection,
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
        info!("Migration `{}` succesfull.", migration.name);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::Hash;
    use sha2::{Digest, Sha256};

    fn test_database_helper() -> Connection {
        Connection::open_in_memory().expect("establish connection to fixture database")
    }

    fn migration_ledger(conn: &Connection) -> Vec<(i64, String, String, i64)> {
        let mut statement = conn
            .prepare(
                r#"
                SELECT
                    version,
                    name,
                    checksum,
                    epoch_us(applied_at)
                FROM senso_rake_schema_migrations
                ORDER BY version
                "#,
            )
            .unwrap();

        statement
            .query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    }

    #[test]
    fn migrating_current_database_is_an_observational_noop() {
        let mut conn = test_database_helper();

        migrate_database(&mut conn).expect("initial migration succeeds");

        conn.execute(
            r#"
            INSERT INTO mappings (
                model,
                id,
                description,
                validity_start
            )
            VALUES (?, ?, ?, TIMESTAMP '2026-01-01 00:00:00')
            "#,
            ("test-model", "test-id", "test sensor"),
        )
        .expect("insert sentinel mapping succeeds");

        let ledger_before = migration_ledger(&conn);

        migrate_database(&mut conn).expect("second migration succeeds");

        let ledger_after = migration_ledger(&conn);

        assert_eq!(ledger_after, ledger_before);

        let mapping_count: i64 = conn
            .query_row(
                r#"
                SELECT count(*)
                FROM mappings
                WHERE model = 'test-model'
                AND id = 'test-id'
                AND description = 'test sensor'
                "#,
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(mapping_count, 1);
    }

    #[test]
    fn unversioned_database_with_senso_rake_table_name_is_rejected_without_changes() {
        let mut conn = test_database_helper();

        conn.execute(
            r#"
            CREATE TABLE mappings (
                mock_col_1 BIGINT PRIMARY KEY,
                mock_col_2 VARCHAR,
                mock_col_3 VARCHAR,
                mock_col_4 TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
            );
            INSERT INTO mappings (
                mock_col_1,
                mock_col_2,
                mock_col_3,
                mock_col_4
            )
            VALUES (?, ?, ?, TIMESTAMP '2026-01-01 00:00:00');
        "#,
            (13, "test-id", "test sensor"),
        )
        .expect("Create non-sensorake table succeeds");

        let res = migrate_database(&mut conn);

        // Migration resulted in an error
        assert_eq!(res, Err(MigrationError::UnversionedDatabase));

        // Select from the original table should succeed
        let mut statement = conn
            .prepare(
                r#"
            SELECT mock_col_1, mock_col_2, mock_col_3, epoch_us(mock_col_4)
            FROM mappings;
        "#,
            )
            .unwrap();

        let row: Vec<(i64, String, String, i64)> = statement
            .query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        // And we should get the same data we inserted before migration
        assert_eq!(
            row,
            vec![(
                13_i64,
                "test-id".into(),
                "test sensor".into(),
                1767225600000000_i64
            )]
        );

        // Verify no migration ledger was created
        assert!(!table_exists(&conn, "senso_rake_schema_migrations").unwrap());
    }

    #[test]
    fn migrate_unversioned_existing_database_without_senso_rake_tables_succeeds() {
        let mut conn = test_database_helper();

        conn.execute(
            r#"
            CREATE TABLE my_test_table (
                mock_col_1 BIGINT PRIMARY KEY,
                mock_col_2 VARCHAR,
                mock_col_3 VARCHAR,
                mock_col_4 TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
            );
            INSERT INTO my_test_table (
                mock_col_1,
                mock_col_2,
                mock_col_3,
                mock_col_4
            )
            VALUES (?, ?, ?, TIMESTAMP '2026-01-01 00:00:00');
        "#,
            (13, "test-id", "test sensor"),
        )
        .expect("Create non-sensorake table succeeds");

        let res = migrate_database(&mut conn);

        // Migration succeeded without an error
        assert_eq!(res, Ok(()));

        // Select from the original table should succeed
        let mut statement = conn
            .prepare(
                r#"
            SELECT mock_col_1, mock_col_2, mock_col_3, epoch_us(mock_col_4)
            FROM my_test_table;
        "#,
            )
            .unwrap();

        let row: Vec<(i64, String, String, i64)> = statement
            .query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        // And we should get the same data we inserted before migration
        assert_eq!(
            row,
            vec![(
                13_i64,
                "test-id".into(),
                "test sensor".into(),
                1767225600000000_i64
            )]
        );

        // insert into migrated mappings table should succeed
        conn.execute(
            r#"
            INSERT INTO mappings (
                model,
                id,
                description,
                validity_start
            )
            VALUES (?, ?, ?, TIMESTAMP '2026-01-01 00:00:00')
            "#,
            ("test-model", "test-id", "test sensor"),
        )
        .expect("insert test mapping succeeds");

        // There should be migration ledger
        let ledger = migration_ledger(&conn);

        // and it should the same size as our migrations definition
        assert_eq!(ledger.len(), MIGRATIONS.len());
    }

    fn test_migrations_helper_two_cases() -> Vec<MigrationDefinition> {
        let sql1 = r#"
                CREATE TABLE senso_rake_schema_migrations (
                    version INTEGER PRIMARY KEY,
                    name TEXT NOT NULL,
                    checksum TEXT NOT NULL,
                    applied_at TIMESTAMP NOT NULL
                );

                CREATE TABLE created_by_first_migration (
                    value INTEGER
                );
            "#;

        let mut hasher = Sha256::new();
        hasher.update(sql1);
        let hash1 = Hash::new(hasher.finalize().into());

        // Should succeed
        let first = MigrationDefinition {
            version: 1,
            name: "create_test_schema",
            hash: hash1,
            sql: sql1,
        };

        let sql2 = r#"
                CREATE TABLE partially_created_table (
                    value INTEGER
                );

                SELECT * FROM table_that_does_not_exist;
            "#;

        hasher = Sha256::new();
        hasher.update(sql2);
        let hash2 = Hash::new(hasher.finalize().into());

        // Should fail
        let second = MigrationDefinition {
            version: 2,
            name: "deliberately_failing_migration",
            hash: hash2,
            sql: sql2,
        };

        let res = vec![first, second];

        for r in &res {
            r.verify_hash().expect("hash should be ok");
        }

        res
    }

    #[test]
    fn migration_failure_rolls_back_schema_and_ledger_changes() {
        let mut conn = test_database_helper();

        let mut migrations = test_migrations_helper_two_cases();

        conn.execute("CREATE TABLE existing_table (value INTEGER)", [])
            .unwrap();

        conn.execute("INSERT INTO existing_table VALUES (42)", [])
            .unwrap();

        let result = migrate_database_with(&mut conn, &mut migrations);

        assert_eq!(result, Err(MigrationError::Database));

        // Objects created inside the failed transaction were removed.
        assert!(!table_exists(&conn, "senso_rake_schema_migrations").unwrap());

        assert!(!table_exists(&conn, "created_by_first_migration").unwrap());

        assert!(!table_exists(&conn, "partially_created_table").unwrap());

        // State that existed before the transaction remains intact.
        let value: i64 = conn
            .query_row("SELECT value FROM existing_table", [], |row| row.get(0))
            .unwrap();

        assert_eq!(value, 42);
    }

    fn test_migrations_helper_three_cases() -> Vec<MigrationDefinition> {
        let sql1 = r#"
                CREATE TABLE senso_rake_schema_migrations (
                    version INTEGER PRIMARY KEY,
                    name TEXT NOT NULL,
                    checksum TEXT NOT NULL,
                    applied_at TIMESTAMP NOT NULL
                );

                CREATE TABLE created_by_first_migration (
                    value INTEGER
                );
            "#;

        let mut hasher = Sha256::new();
        hasher.update(sql1);
        let hash1 = Hash::new(hasher.finalize().into());

        let first = MigrationDefinition {
            version: 1,
            name: "create_test_schema",
            hash: hash1,
            sql: sql1,
        };

        let sql2 = r#"
                CREATE TABLE partially_created_table (
                    value INTEGER
                );
            "#;

        hasher = Sha256::new();
        hasher.update(sql2);
        let hash2 = Hash::new(hasher.finalize().into());

        let second = MigrationDefinition {
            version: 2,
            name: "second_migration",
            hash: hash2,
            sql: sql2,
        };

        let sql3 = r#"
                CREATE TABLE created_by_migration_3 (
                    value1 INTEGER,
                    value2 VARCHAR,
                );
            "#;

        hasher = Sha256::new();
        hasher.update(sql3);
        let hash3 = Hash::new(hasher.finalize().into());

        let third = MigrationDefinition {
            version: 3,
            name: "third_migration_with_sentinel_table",
            hash: hash3,
            sql: sql3,
        };

        let res = vec![first, second, third];

        for r in &res {
            r.verify_hash().expect("hash should be ok");
        }

        res
    }

    #[test]
    fn database_migrated_by_newer_application_is_rejected_without_changes() {
        let mut conn = test_database_helper();

        let mut newer_definitions = test_migrations_helper_three_cases();
        assert_eq!(newer_definitions.len(), 3);

        migrate_database_with(&mut conn, &mut newer_definitions)
            .expect("newer migration set succeeds");

        let ledger_before = migration_ledger(&conn);

        let mut older_definitions = newer_definitions[..2].to_vec();

        let result = migrate_database_with(&mut conn, &mut older_definitions);

        assert_eq!(result, Err(MigrationError::TooManyMigrations));
        assert_eq!(migration_ledger(&conn), ledger_before);
        assert!(table_exists(&conn, "created_by_migration_3").unwrap());
    }

    #[test]
    fn database_with_valid_migration_prefix_applies_only_remaining_migrations() {
        let mut conn = test_database_helper();

        let all_definitions = test_migrations_helper_three_cases();
        let mut first_two = all_definitions[..2].to_vec();

        migrate_database_with(&mut conn, &mut first_two).expect("initial migrations succeed");

        let ledger_before = migration_ledger(&conn);
        assert_eq!(ledger_before.len(), 2);
        assert!(!table_exists(&conn, "created_by_migration_3").unwrap());

        let mut all_definitions = all_definitions;

        migrate_database_with(&mut conn, &mut all_definitions)
            .expect("remaining migration succeeds");

        let ledger_after = migration_ledger(&conn);

        assert_eq!(ledger_after.len(), 3);
        assert_eq!(&ledger_after[..2], ledger_before.as_slice());
        assert!(table_exists(&conn, "created_by_migration_3").unwrap());
    }

    #[test]
    fn modified_applied_migration_is_rejected_without_changes() {
        let mut conn = test_database_helper();
        let mut migrations = test_migrations_helper_three_cases();

        migrate_database_with(&mut conn, &mut migrations).expect("initial migration succeeds");

        conn.execute(
            r#"
        UPDATE senso_rake_schema_migrations
        SET checksum = ?
        WHERE version = 2
        "#,
            ("0000000000000000000000000000000000000000000000000000000000000000",),
        )
        .unwrap();

        let ledger_before = migration_ledger(&conn);

        let result = migrate_database_with(&mut conn, &mut migrations);

        assert_eq!(result, Err(MigrationError::MigrationMismatch));
        assert_eq!(migration_ledger(&conn), ledger_before);
    }
}
