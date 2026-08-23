mod migrations;

use std::{
    fs,
    path::{Path, PathBuf},
    str::FromStr,
};

use rusqlite::{Connection, OptionalExtension};
use thiserror::Error;

use crate::config::DataEnvironment;

const ENVIRONMENT_KEY: &str = "data_environment";

pub struct Database {
    connection: Connection,
}

impl Database {
    pub fn open(path: &Path, environment: DataEnvironment) -> Result<Self, DatabaseError> {
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent).map_err(|source| DatabaseError::CreateDirectory {
                path: parent.to_path_buf(),
                source,
            })?;
        }

        let connection = Connection::open(path)?;
        Self::initialize(connection, environment, Some(path))
    }

    pub fn open_in_memory(environment: DataEnvironment) -> Result<Self, DatabaseError> {
        let connection = Connection::open_in_memory()?;
        Self::initialize(connection, environment, None)
    }

    fn initialize(
        mut connection: Connection,
        expected_environment: DataEnvironment,
        path: Option<&Path>,
    ) -> Result<Self, DatabaseError> {
        connection.execute_batch(
            "PRAGMA foreign_keys = ON;\n\
             PRAGMA busy_timeout = 5000;",
        )?;
        migrations::run(&mut connection)?;

        let stored_environment: Option<String> = connection
            .query_row(
                "SELECT value FROM app_metadata WHERE key = ?1",
                [ENVIRONMENT_KEY],
                |row| row.get(0),
            )
            .optional()?;

        match stored_environment {
            Some(stored) => {
                let actual = DataEnvironment::from_str(&stored)
                    .map_err(|_| DatabaseError::InvalidEnvironment(stored.clone()))?;
                if actual != expected_environment {
                    return Err(DatabaseError::EnvironmentMismatch {
                        expected: expected_environment,
                        actual,
                        path: path.map(Path::to_path_buf),
                    });
                }
            }
            None => {
                connection.execute(
                    "INSERT INTO app_metadata (key, value) VALUES (?1, ?2)",
                    (ENVIRONMENT_KEY, expected_environment.as_str()),
                )?;
            }
        }

        Ok(Self { connection })
    }

    pub fn environment_identity(&self) -> Result<&str, DatabaseError> {
        let value: String = self.connection.query_row(
            "SELECT value FROM app_metadata WHERE key = ?1",
            [ENVIRONMENT_KEY],
            |row| row.get(0),
        )?;

        match value.as_str() {
            "personal" => Ok("personal"),
            "development" => Ok("development"),
            "test" => Ok("test"),
            _ => Err(DatabaseError::InvalidEnvironment(value)),
        }
    }

    pub fn schema_version(&self) -> Result<i64, DatabaseError> {
        Ok(self.connection.query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |row| row.get(0),
        )?)
    }

    #[cfg(test)]
    pub(crate) fn connection(&self) -> &Connection {
        &self.connection
    }
}

#[derive(Debug, Error)]
pub enum DatabaseError {
    #[error("SQLite operation failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("could not create database directory {path}: {source}")]
    CreateDirectory {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("database contains an invalid environment identity '{0}'")]
    InvalidEnvironment(String),
    #[error(
        "refusing to open {actual:?} database as {expected:?}{path_suffix}",
        path_suffix = .path.as_ref().map(|p| format!(" at {}", p.display())).unwrap_or_default()
    )]
    EnvironmentMismatch {
        expected: DataEnvironment,
        actual: DataEnvironment,
        path: Option<PathBuf>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_memory_database_is_migrated_and_tagged_for_tests() {
        let database =
            Database::open_in_memory(DataEnvironment::Test).expect("database should initialize");

        assert_eq!(database.environment_identity().unwrap(), "test");
        assert_eq!(database.schema_version().unwrap(), 1);
        assert_eq!(
            database
                .connection()
                .query_row("PRAGMA foreign_keys", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            1
        );
    }

    #[test]
    fn persistent_database_reopens_with_matching_identity() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("ippo.db");

        Database::open(&path, DataEnvironment::Development).expect("first open");
        let reopened =
            Database::open(&path, DataEnvironment::Development).expect("matching reopen");

        assert_eq!(reopened.environment_identity().unwrap(), "development");
        assert_eq!(reopened.schema_version().unwrap(), 1);
    }

    #[test]
    fn persistent_database_rejects_environment_mismatch() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("ippo.db");

        Database::open(&path, DataEnvironment::Development).expect("development database");
        let error = Database::open(&path, DataEnvironment::Personal)
            .err()
            .expect("personal open must fail");

        assert!(matches!(
            error,
            DatabaseError::EnvironmentMismatch {
                expected: DataEnvironment::Personal,
                actual: DataEnvironment::Development,
                ..
            }
        ));
    }
}
