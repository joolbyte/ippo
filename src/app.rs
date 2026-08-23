use jiff::{Timestamp, civil::Date};
use thiserror::Error;

use crate::{
    clock::{Clock, TimeZoneSource},
    habit::{HabitName, HabitNameError, TodayHabit},
    storage::{Database, DatabaseError},
};

pub struct HabitApplication<C, T> {
    database: Database,
    clock: C,
    time_zone: T,
    today: Date,
    timezone_name: String,
    habits: Vec<TodayHabit>,
}

impl<C: Clock, T: TimeZoneSource> HabitApplication<C, T> {
    pub fn new(database: Database, clock: C, time_zone: T) -> Result<Self, ApplicationError> {
        let (now, today, timezone_name) = current_context(&clock, &time_zone)?;
        let mut application = Self {
            database,
            clock,
            time_zone,
            today,
            timezone_name,
            habits: Vec::new(),
        };
        application.reload(now)?;
        Ok(application)
    }

    pub fn today(&self) -> Date {
        self.today
    }

    pub fn habits(&self) -> &[TodayHabit] {
        &self.habits
    }

    pub fn completed_count(&self) -> usize {
        self.habits.iter().filter(|habit| habit.completed).count()
    }

    pub fn completion_percentage(&self) -> u16 {
        if self.habits.is_empty() {
            0
        } else {
            ((self.completed_count() * 100) / self.habits.len()) as u16
        }
    }

    pub fn create_daily_binary(&mut self, name: &str) -> Result<(), ApplicationError> {
        let name = HabitName::parse(name)?;
        let now = self.clock.now();
        self.database.create_daily_binary_habit(
            &name,
            &self.today.to_string(),
            &self.timezone_name,
            &now.to_string(),
        )?;
        self.reload(now)
    }

    pub fn toggle(&mut self, occurrence_id: i64) -> Result<(), ApplicationError> {
        let now = self.clock.now();
        self.database.toggle_binary_occurrence(
            occurrence_id,
            &self.today.to_string(),
            &now.to_string(),
        )?;
        self.reload(now)
    }

    pub fn refresh_day(&mut self) -> Result<bool, ApplicationError> {
        let (now, today, timezone_name) = current_context(&self.clock, &self.time_zone)?;
        if today == self.today && timezone_name == self.timezone_name {
            return Ok(false);
        }

        self.today = today;
        self.timezone_name = timezone_name;
        self.reload(now)?;
        Ok(true)
    }

    fn reload(&mut self, now: Timestamp) -> Result<(), ApplicationError> {
        let date = self.today.to_string();
        self.database.materialize_daily_occurrences(
            &date,
            &self.timezone_name,
            &now.to_string(),
        )?;
        self.habits = self.database.today_habits(&date)?;
        Ok(())
    }
}

fn current_context(
    clock: &impl Clock,
    time_zone: &impl TimeZoneSource,
) -> Result<(Timestamp, Date, String), ApplicationError> {
    let now = clock.now();
    let time_zone = time_zone.time_zone();
    let timezone_name = time_zone
        .iana_name()
        .ok_or(ApplicationError::UnnamedTimeZone)?
        .to_owned();
    let today = now.to_zoned(time_zone).date();
    Ok((now, today, timezone_name))
}

#[derive(Debug, Error)]
pub enum ApplicationError {
    #[error(transparent)]
    Database(#[from] DatabaseError),
    #[error(transparent)]
    InvalidHabitName(#[from] HabitNameError),
    #[error("the active timezone has no IANA identifier")]
    UnnamedTimeZone,
}

#[cfg(test)]
mod tests {
    use jiff::{Timestamp, civil::date, tz::TimeZone};

    use super::*;
    use crate::{
        clock::{FixedClock, FixedTimeZone},
        config::DataEnvironment,
    };

    #[test]
    fn binary_habit_and_completion_survive_restart() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("ippo.db");
        let timestamp: Timestamp = "2026-08-23T12:00:00Z".parse().unwrap();

        {
            let database = Database::open(&path, DataEnvironment::Test).unwrap();
            let mut application = HabitApplication::new(
                database,
                FixedClock::new(timestamp),
                FixedTimeZone::new(TimeZone::UTC),
            )
            .unwrap();

            application.create_daily_binary("read").unwrap();
            assert_eq!(application.today(), date(2026, 8, 23));
            assert_eq!(application.habits().len(), 1);
            assert!(!application.habits()[0].completed);

            application
                .toggle(application.habits()[0].occurrence_id)
                .unwrap();
            assert!(application.habits()[0].completed);
            assert_eq!(application.completion_percentage(), 100);
        }

        let database = Database::open(&path, DataEnvironment::Test).unwrap();
        let reopened = HabitApplication::new(
            database,
            FixedClock::new(timestamp),
            FixedTimeZone::new(TimeZone::UTC),
        )
        .unwrap();

        assert_eq!(reopened.habits().len(), 1);
        assert_eq!(reopened.habits()[0].name, "read");
        assert!(reopened.habits()[0].completed);
    }
}
