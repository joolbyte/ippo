use rusqlite::{Connection, OptionalExtension};

use super::DatabaseError;

struct Migration {
    version: i64,
    description: &'static str,
    sql: &'static str,
}

const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    description: "foundation metadata",
    sql: include_str!("../../migrations/0001_foundation.sql"),
}];

pub(super) fn run(connection: &mut Connection) -> Result<(), DatabaseError> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (\n\
             version     INTEGER PRIMARY KEY NOT NULL,\n\
             description TEXT NOT NULL,\n\
             applied_at  TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP\n\
         ) STRICT;",
    )?;

    for migration in MIGRATIONS {
        let applied = connection
            .query_row(
                "SELECT 1 FROM schema_migrations WHERE version = ?1",
                [migration.version],
                |_| Ok(()),
            )
            .optional()?
            .is_some();

        if applied {
            continue;
        }

        let transaction = connection.transaction()?;
        transaction.execute_batch(migration.sql)?;
        transaction.execute(
            "INSERT INTO schema_migrations (version, description) VALUES (?1, ?2)",
            (migration.version, migration.description),
        )?;
        transaction.commit()?;
    }

    Ok(())
}
