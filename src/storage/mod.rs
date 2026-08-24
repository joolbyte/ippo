mod migrations;

use std::{
    fs,
    path::{Path, PathBuf},
    str::FromStr,
};

use jiff::civil::Date;
use rusqlite::{Connection, OptionalExtension, params};
use thiserror::Error;

use crate::{
    config::DataEnvironment,
    habit::{DayProgress, HabitName, Routine, RoutineName, TodayHabit},
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

    pub fn create_routine(
        &mut self,
        name: &RoutineName,
        now_utc: &str,
    ) -> Result<(), DatabaseError> {
        let exists = self.connection.query_row(
            "SELECT EXISTS (
                 SELECT 1 FROM routines
                 WHERE name = ?1 COLLATE NOCASE AND archived_at_utc IS NULL
             )",
            [name.as_str()],
            |row| row.get::<_, bool>(0),
        )?;
        if exists {
            return Err(DatabaseError::RoutineAlreadyExists(
                name.as_str().to_owned(),
            ));
        }

        self.connection.execute(
            "INSERT INTO routines (name, created_at_utc) VALUES (?1, ?2)",
            (name.as_str(), now_utc),
        )?;
        Ok(())
    }

    pub fn routines(&self) -> Result<Vec<Routine>, DatabaseError> {
        let mut statement = self.connection.prepare(
            "SELECT id, name
             FROM routines
             WHERE archived_at_utc IS NULL
             ORDER BY created_at_utc, id",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(Routine {
                id: row.get(0)?,
                name: row.get(1)?,
            })
        })?;

        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn update_habit_settings(
        &mut self,
        habit_id: i64,
        name: &HabitName,
        routine_ids: &[i64],
        active_date: &str,
        now_utc: &str,
    ) -> Result<(), DatabaseError> {
        let transaction = self.connection.transaction()?;
        let updated = transaction.execute(
            "UPDATE habits SET name = ?1 WHERE id = ?2 AND archived_at_utc IS NULL",
            params![name.as_str(), habit_id],
        )?;
        if updated == 0 {
            return Err(DatabaseError::HabitNotFound(habit_id));
        }

        transaction.execute(
            "UPDATE habit_occurrences
             SET habit_name = ?1, updated_at_utc = ?2
             WHERE habit_id = ?3 AND scheduled_date = ?4",
            params![name.as_str(), now_utc, habit_id, active_date],
        )?;
        transaction.execute("DELETE FROM habit_routines WHERE habit_id = ?1", [habit_id])?;

        for (position, routine_id) in routine_ids.iter().enumerate() {
            let inserted = transaction.execute(
                "INSERT INTO habit_routines (habit_id, routine_id, position, created_at_utc)
                 SELECT ?1, id, ?2, ?3
                 FROM routines
                 WHERE id = ?4 AND archived_at_utc IS NULL",
                params![habit_id, position as i64, now_utc, routine_id],
            )?;
            if inserted == 0 {
                return Err(DatabaseError::RoutineNotFound(*routine_id));
            }
        }

        let occurrence_id: Option<i64> = transaction
            .query_row(
                "SELECT id FROM habit_occurrences
                 WHERE habit_id = ?1 AND scheduled_date = ?2",
                params![habit_id, active_date],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(occurrence_id) = occurrence_id {
            transaction.execute(
                "DELETE FROM habit_occurrence_routines WHERE occurrence_id = ?1",
                [occurrence_id],
            )?;
            transaction.execute(
                "INSERT INTO habit_occurrence_routines (
                     occurrence_id, routine_id, routine_name, position
                 )
                 SELECT ?1, r.id, r.name, hr.position
                 FROM habit_routines hr
                 JOIN routines r ON r.id = hr.routine_id
                 WHERE hr.habit_id = ?2
                 ORDER BY hr.position, r.id",
                params![occurrence_id, habit_id],
            )?;
        }

        transaction.commit()?;
        Ok(())
    }

    pub fn materialize_daily_occurrences(
        &mut self,
        date: &str,
        timezone: &str,
        now_utc: &str,
    ) -> Result<(), DatabaseError> {
        let transaction = self.connection.transaction()?;
        let due = {
            let mut statement = transaction.prepare(
                "SELECT s.id, h.id, h.name, h.habit_type
                 FROM habits h
                 JOIN habit_schedules s ON s.habit_id = h.id
                 WHERE h.archived_at_utc IS NULL
                   AND s.schedule_kind = 'daily'
                   AND s.starts_on <= ?1
                   AND (s.ends_on IS NULL OR s.ends_on >= ?1)
                 ORDER BY h.created_at_utc, h.id",
            )?;
            let rows = statement.query_map([date], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };

        for (schedule_id, habit_id, habit_name, habit_type) in due {
            let inserted = transaction.execute(
                "INSERT INTO habit_occurrences (
                     schedule_id, habit_id, scheduled_date, timezone, habit_name, habit_type,
                     completed, completed_at_utc, created_at_utc, updated_at_utc
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, NULL, ?7, ?7)
                 ON CONFLICT (habit_id, scheduled_date) DO NOTHING",
                params![
                    schedule_id,
                    habit_id,
                    date,
                    timezone,
                    habit_name,
                    habit_type,
                    now_utc
                ],
            )?;

            if inserted == 1 {
                let occurrence_id = transaction.last_insert_rowid();
                transaction.execute(
                    "INSERT INTO habit_occurrence_routines (
                         occurrence_id, routine_id, routine_name, position
                     )
                     SELECT ?1, r.id, r.name, hr.position
                     FROM habit_routines hr
                     JOIN routines r ON r.id = hr.routine_id
                     WHERE hr.habit_id = ?2
                     ORDER BY hr.position, r.id",
                    params![occurrence_id, habit_id],
                )?;
            }
        }

        transaction.commit()?;
        Ok(())
    }

    pub fn reconcile_daily_occurrences_through(
        &mut self,
        through_date: Date,
        timezone: &str,
        now_utc: &str,
    ) -> Result<(), DatabaseError> {
        let first_date: Option<String> = self.connection.query_row(
            "SELECT MIN(starts_on) FROM habit_schedules
             WHERE schedule_kind = 'daily' AND starts_on <= ?1",
            [through_date.to_string()],
            |row| row.get(0),
        )?;
        let Some(first_date) = first_date else {
            return Ok(());
        };
        let mut date = Date::from_str(&first_date)
            .map_err(|_| DatabaseError::InvalidStoredDate(first_date.clone()))?;

        while date <= through_date {
            self.materialize_daily_occurrences(&date.to_string(), timezone, now_utc)?;
            if date == through_date {
                break;
            }
            date = date
                .tomorrow()
                .map_err(|_| DatabaseError::InvalidStoredDate(date.to_string()))?;
        }
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
                routines: Vec::new(),
            })
        })?;
        let mut habits = rows.collect::<Result<Vec<_>, _>>()?;
        drop(statement);

        let mut routine_statement = self.connection.prepare(
            "SELECT routine_id, routine_name
             FROM habit_occurrence_routines
             WHERE occurrence_id = ?1
             ORDER BY position, routine_id",
        )?;
        for habit in &mut habits {
            let routines = routine_statement.query_map([habit.occurrence_id], |row| {
                Ok(Routine {
                    id: row.get(0)?,
                    name: row.get(1)?,
                })
            })?;
            habit.routines = routines.collect::<Result<Vec<_>, _>>()?;
        }

        Ok(habits)
    }

    pub fn day_progress(
        &self,
        start_date: &str,
        end_date: &str,
    ) -> Result<Vec<DayProgress>, DatabaseError> {
        let mut statement = self.connection.prepare(
            "SELECT scheduled_date, COUNT(*), SUM(completed)
             FROM habit_occurrences
             WHERE scheduled_date BETWEEN ?1 AND ?2
             GROUP BY scheduled_date
             ORDER BY scheduled_date",
        )?;
        let rows = statement.query_map(params![start_date, end_date], |row| {
            let date: String = row.get(0)?;
            Ok((date, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?))
        })?;

        rows.map(|row| {
            let (date, scheduled, completed) = row?;
            let parsed = Date::from_str(&date)
                .map_err(|_| DatabaseError::InvalidStoredDate(date.clone()))?;
            Ok(DayProgress {
                date: parsed,
                scheduled: scheduled as usize,
                completed: completed as usize,
            })
        })
        .collect()
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
    #[error("habit {0} was not found")]
    HabitNotFound(i64),
    #[error("routine {0} was not found")]
    RoutineNotFound(i64),
    #[error("a routine named '{0}' already exists")]
    RoutineAlreadyExists(String),
    #[error("database contains an invalid civil date '{0}'")]
    InvalidStoredDate(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_memory_database_is_migrated_and_tagged_for_tests() {
        let database =
            Database::open_in_memory(DataEnvironment::Test).expect("database should initialize");

        assert_eq!(database.environment_identity().unwrap(), "test");
        assert_eq!(database.schema_version().unwrap(), 3);
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
        assert_eq!(reopened.schema_version().unwrap(), 3);
    }

    #[test]
    fn version_two_database_migrates_without_losing_habits() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("ippo.db");
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE schema_migrations (
                     version INTEGER PRIMARY KEY NOT NULL,
                     description TEXT NOT NULL,
                     applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                 ) STRICT;",
            )
            .unwrap();
        connection
            .execute_batch(include_str!("../../migrations/0001_foundation.sql"))
            .unwrap();
        connection
            .execute_batch(include_str!("../../migrations/0002_binary_habits.sql"))
            .unwrap();
        connection
            .execute(
                "INSERT INTO schema_migrations (version, description)
                 VALUES (1, 'foundation metadata'), (2, 'daily binary habits and occurrences')",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO app_metadata (key, value) VALUES (?1, ?2)",
                (ENVIRONMENT_KEY, "development"),
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO habits (name, habit_type, created_at_utc)
                 VALUES ('read', 'binary', '2026-08-23T08:00:00Z')",
                [],
            )
            .unwrap();
        drop(connection);

        let migrated = Database::open(&path, DataEnvironment::Development).unwrap();

        assert_eq!(migrated.schema_version().unwrap(), 3);
        assert_eq!(
            migrated
                .connection()
                .query_row("SELECT name FROM habits", [], |row| row.get::<_, String>(0))
                .unwrap(),
            "read"
        );
        assert!(migrated.routines().unwrap().is_empty());
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

    #[test]
    fn habit_settings_preserve_past_names_and_routine_membership() {
        let mut database =
            Database::open_in_memory(DataEnvironment::Test).expect("database should initialize");
        database
            .create_daily_binary_habit(
                &HabitName::parse("read").unwrap(),
                "2026-08-23",
                "Europe/Oslo",
                "2026-08-23T08:00:00Z",
            )
            .unwrap();
        database
            .create_routine(
                &RoutineName::parse("morning").unwrap(),
                "2026-08-23T08:01:00Z",
            )
            .unwrap();
        let habit_id = database.today_habits("2026-08-23").unwrap()[0].habit_id;
        let morning_id = database.routines().unwrap()[0].id;
        database
            .update_habit_settings(
                habit_id,
                &HabitName::parse("read").unwrap(),
                &[morning_id],
                "2026-08-23",
                "2026-08-23T08:02:00Z",
            )
            .unwrap();
        database
            .materialize_daily_occurrences("2026-08-24", "Europe/Oslo", "2026-08-24T08:00:00Z")
            .unwrap();
        database
            .create_routine(
                &RoutineName::parse("evening").unwrap(),
                "2026-08-24T08:01:00Z",
            )
            .unwrap();
        let evening_id = database.routines().unwrap()[1].id;
        database
            .update_habit_settings(
                habit_id,
                &HabitName::parse("write").unwrap(),
                &[evening_id],
                "2026-08-24",
                "2026-08-24T08:02:00Z",
            )
            .unwrap();

        let first_day = database.today_habits("2026-08-23").unwrap();
        assert_eq!(first_day[0].name, "read");
        assert_eq!(first_day[0].routines[0].name, "morning");
        let second_day = database.today_habits("2026-08-24").unwrap();
        assert_eq!(second_day[0].name, "write");
        assert_eq!(second_day[0].routines[0].name, "evening");
    }

    #[test]
    fn reconciliation_backfills_missed_daily_occurrences_for_contributions() {
        let mut database =
            Database::open_in_memory(DataEnvironment::Test).expect("database should initialize");
        database
            .create_daily_binary_habit(
                &HabitName::parse("read").unwrap(),
                "2026-08-20",
                "Europe/Oslo",
                "2026-08-20T08:00:00Z",
            )
            .unwrap();

        database
            .reconcile_daily_occurrences_through(
                Date::new(2026, 8, 23).unwrap(),
                "Europe/Oslo",
                "2026-08-23T08:00:00Z",
            )
            .unwrap();

        let progress = database.day_progress("2026-08-20", "2026-08-23").unwrap();
        assert_eq!(progress.len(), 4);
        assert!(progress.iter().all(|day| day.scheduled == 1));
        assert!(progress.iter().all(|day| day.percentage() == 0));
    }
}
