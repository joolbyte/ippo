mod migrations;

use std::{
    fs,
    path::{Path, PathBuf},
    str::FromStr,
};

use rusqlite::{Connection, OptionalExtension, params};
use thiserror::Error;

use crate::{
    config::DataEnvironment,
    habit::{HabitName, TodayHabit},
};

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

    pub fn create_daily_binary_habit(
        &mut self,
        name: &HabitName,
        starts_on: &str,
        timezone: &str,
        now_utc: &str,
    ) -> Result<(), DatabaseError> {
        let transaction = self.connection.transaction()?;
        transaction.execute(
            "INSERT INTO habits (name, habit_type, created_at_utc)
             VALUES (?1, 'binary', ?2)",
            (name.as_str(), now_utc),
        )?;
        let habit_id = transaction.last_insert_rowid();

        transaction.execute(
            "INSERT INTO habit_schedules (
                 habit_id, schedule_kind, starts_on, created_timezone, created_at_utc
             ) VALUES (?1, 'daily', ?2, ?3, ?4)",
            params![habit_id, starts_on, timezone, now_utc],
        )?;
        let schedule_id = transaction.last_insert_rowid();

        transaction.execute(
            "INSERT INTO habit_occurrences (
                 schedule_id, habit_id, scheduled_date, timezone, habit_name, habit_type,
                 completed, completed_at_utc, created_at_utc, updated_at_utc
             ) VALUES (?1, ?2, ?3, ?4, ?5, 'binary', 0, NULL, ?6, ?6)",
            params![
                schedule_id,
                habit_id,
                starts_on,
                timezone,
                name.as_str(),
                now_utc
            ],
        )?;

        transaction.commit()?;
        Ok(())
    }

    pub fn materialize_daily_occurrences(
        &mut self,
        date: &str,
        timezone: &str,
        now_utc: &str,
    ) -> Result<(), DatabaseError> {
        self.connection.execute(
            "INSERT INTO habit_occurrences (
                 schedule_id, habit_id, scheduled_date, timezone, habit_name, habit_type,
                 completed, completed_at_utc, created_at_utc, updated_at_utc
             )
             SELECT s.id, h.id, ?1, ?2, h.name, h.habit_type, 0, NULL, ?3, ?3
             FROM habits h
             JOIN habit_schedules s ON s.habit_id = h.id
             WHERE h.archived_at_utc IS NULL
               AND s.schedule_kind = 'daily'
               AND s.starts_on <= ?1
               AND (s.ends_on IS NULL OR s.ends_on >= ?1)
             ON CONFLICT (habit_id, scheduled_date) DO NOTHING",
            params![date, timezone, now_utc],
        )?;
        Ok(())
    }

    pub fn today_habits(&self, date: &str) -> Result<Vec<TodayHabit>, DatabaseError> {
        let mut statement = self.connection.prepare(
            "SELECT o.id, o.habit_id, o.habit_name, o.completed
             FROM habit_occurrences o
             JOIN habits h ON h.id = o.habit_id
             WHERE o.scheduled_date = ?1
               AND h.archived_at_utc IS NULL
             ORDER BY o.completed ASC, h.created_at_utc, h.id",
        )?;
        let rows = statement.query_map([date], |row| {
            Ok(TodayHabit {
                occurrence_id: row.get(0)?,
                habit_id: row.get(1)?,
                name: row.get(2)?,
                completed: row.get::<_, i64>(3)? != 0,
            })
        })?;

        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn toggle_binary_occurrence(
        &mut self,
        occurrence_id: i64,
        expected_date: &str,
        now_utc: &str,
    ) -> Result<(), DatabaseError> {
        let transaction = self.connection.transaction()?;
        let completed: Option<i64> = transaction
            .query_row(
                "SELECT completed
                 FROM habit_occurrences
                 WHERE id = ?1 AND scheduled_date = ?2 AND habit_type = 'binary'",
                params![occurrence_id, expected_date],
                |row| row.get(0),
            )
            .optional()?;
        let completed = completed.ok_or(DatabaseError::OccurrenceNotFound(occurrence_id))?;
        let next_completed = i64::from(completed == 0);
        let completed_at = (next_completed == 1).then_some(now_utc);

        transaction.execute(
            "UPDATE habit_occurrences
             SET completed = ?1, completed_at_utc = ?2, updated_at_utc = ?3
             WHERE id = ?4",
            params![next_completed, completed_at, now_utc, occurrence_id],
        )?;
        transaction.commit()?;
        Ok(())
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
    #[error("habit occurrence {0} was not found for the active day")]
    OccurrenceNotFound(i64),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_memory_database_is_migrated_and_tagged_for_tests() {
        let database =
            Database::open_in_memory(DataEnvironment::Test).expect("database should initialize");

        assert_eq!(database.environment_identity().unwrap(), "test");
        assert_eq!(database.schema_version().unwrap(), 2);
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
        assert_eq!(reopened.schema_version().unwrap(), 2);
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

    #[test]
    fn daily_occurrences_materialize_once_and_toggle_transactionally() {
        let mut database =
            Database::open_in_memory(DataEnvironment::Test).expect("database should initialize");
        let name = HabitName::parse("read").unwrap();

        database
            .create_daily_binary_habit(&name, "2026-08-23", "Europe/Oslo", "2026-08-23T08:00:00Z")
            .unwrap();
        database
            .materialize_daily_occurrences("2026-08-23", "Europe/Oslo", "2026-08-23T08:00:01Z")
            .unwrap();

        let habits = database.today_habits("2026-08-23").unwrap();
        assert_eq!(habits.len(), 1);
        assert_eq!(habits[0].name, "read");
        assert!(!habits[0].completed);

        database
            .toggle_binary_occurrence(
                habits[0].occurrence_id,
                "2026-08-23",
                "2026-08-23T08:05:00Z",
            )
            .unwrap();
        assert!(database.today_habits("2026-08-23").unwrap()[0].completed);
    }

    #[test]
    fn completed_occurrences_follow_unchecked_occurrences() {
        let mut database =
            Database::open_in_memory(DataEnvironment::Test).expect("database should initialize");

        for name in ["first", "second", "third"] {
            database
                .create_daily_binary_habit(
                    &HabitName::parse(name).unwrap(),
                    "2026-08-23",
                    "Europe/Oslo",
                    "2026-08-23T08:00:00Z",
                )
                .unwrap();
        }

        let second_occurrence = database.today_habits("2026-08-23").unwrap()[1].occurrence_id;
        database
            .toggle_binary_occurrence(second_occurrence, "2026-08-23", "2026-08-23T08:05:00Z")
            .unwrap();

        let habits = database.today_habits("2026-08-23").unwrap();
        let names: Vec<_> = habits.iter().map(|habit| habit.name.as_str()).collect();
        let completions: Vec<_> = habits.iter().map(|habit| habit.completed).collect();

        assert_eq!(names, ["first", "third", "second"]);
        assert_eq!(completions, [false, false, true]);
    }
}
