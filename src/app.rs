use jiff::{Span, Timestamp, civil::Date};
use thiserror::Error;

use crate::{
    clock::{Clock, TimeZoneSource},
    habit::{
        DayProgress, HabitName, HabitNameError, ProjectedHabit, Routine, RoutineName,
        RoutineNameError, TodayHabit,
    },
    storage::{Database, DatabaseError},
};

pub struct HabitApplication<C, T> {
    database: Database,
    clock: C,
    time_zone: T,
    today: Date,
    selected_date: Date,
    timezone_name: String,
    habits: Vec<TodayHabit>,
    projected_habits: Vec<ProjectedHabit>,
    routines: Vec<Routine>,
    contributions: Vec<DayProgress>,
}

impl<C: Clock, T: TimeZoneSource> HabitApplication<C, T> {
    pub fn new(database: Database, clock: C, time_zone: T) -> Result<Self, ApplicationError> {
        let (now, today, timezone_name) = current_context(&clock, &time_zone)?;
        let mut application = Self {
            database,
            clock,
            time_zone,
            today,
            selected_date: today,
            timezone_name,
            habits: Vec::new(),
            projected_habits: Vec::new(),
            routines: Vec::new(),
            contributions: Vec::new(),
        };
        application.reload(now, true)?;
        Ok(application)
    }

    pub fn today(&self) -> Date {
        self.today
    }

    pub fn habits(&self) -> &[TodayHabit] {
        &self.habits
    }

    pub fn selected_date(&self) -> Date {
        self.selected_date
    }

    pub fn projected_habits(&self) -> &[ProjectedHabit] {
        &self.projected_habits
    }

    pub fn routines(&self) -> &[Routine] {
        &self.routines
    }

    pub fn contributions(&self) -> &[DayProgress] {
        &self.contributions
    }

    pub fn is_viewing_today(&self) -> bool {
        self.selected_date == self.today
    }

    pub fn is_viewing_future(&self) -> bool {
        self.selected_date > self.today
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
        self.require_today()?;
        let name = HabitName::parse(name)?;
        let now = self.clock.now();
        self.database.create_daily_binary_habit(
            &name,
            &self.today.to_string(),
            &self.timezone_name,
            &now.to_string(),
        )?;
        self.reload(now, false)
    }

    pub fn create_routine(&mut self, name: &str) -> Result<(), ApplicationError> {
        self.require_today()?;
        let name = RoutineName::parse(name)?;
        let now = self.clock.now();
        self.database.create_routine(&name, &now.to_string())?;
        self.reload(now, false)
    }

    pub fn update_habit_settings(
        &mut self,
        habit_id: i64,
        name: &str,
        routine_ids: &[i64],
    ) -> Result<(), ApplicationError> {
        self.require_today()?;
        let name = HabitName::parse(name)?;
        let now = self.clock.now();
        self.database.update_habit_settings(
            habit_id,
            &name,
            routine_ids,
            &self.today.to_string(),
            &now.to_string(),
        )?;
        self.reload(now, false)
    }

    pub fn toggle(&mut self, occurrence_id: i64) -> Result<(), ApplicationError> {
        self.require_today()?;
        let now = self.clock.now();
        self.database.toggle_binary_occurrence(
            occurrence_id,
            &self.today.to_string(),
            &now.to_string(),
        )?;
        self.reload(now, false)
    }

    pub fn select_date(&mut self, date: Date) -> Result<(), ApplicationError> {
        self.selected_date = date;
        self.reload(self.clock.now(), false)
    }

    pub fn refresh_day(&mut self) -> Result<bool, ApplicationError> {
        let (now, today, timezone_name) = current_context(&self.clock, &self.time_zone)?;
        if today == self.today && timezone_name == self.timezone_name {
            return Ok(false);
        }

        let was_viewing_today = self.selected_date == self.today;
        self.today = today;
        if was_viewing_today {
            self.selected_date = today;
        }
        self.timezone_name = timezone_name;
        self.reload(now, true)?;
        Ok(true)
    }

    fn require_today(&self) -> Result<(), ApplicationError> {
        if self.is_viewing_today() {
            Ok(())
        } else if self.is_viewing_future() {
            Err(ApplicationError::ReadOnlyFuture(self.selected_date))
        } else {
            Err(ApplicationError::ReadOnlyHistory(self.selected_date))
        }
    }

    fn reload(&mut self, now: Timestamp, reconcile: bool) -> Result<(), ApplicationError> {
        if reconcile {
            self.database.reconcile_daily_occurrences_through(
                self.today,
                &self.timezone_name,
                &now.to_string(),
            )?;
        }
        if self.is_viewing_future() {
            self.habits.clear();
            self.projected_habits = self
                .database
                .projected_habits(&self.selected_date.to_string())?;
        } else {
            self.habits = self
                .database
                .today_habits(&self.selected_date.to_string())?;
            self.projected_habits.clear();
        }
        self.routines = self.database.routines()?;
        let contribution_start = self.today.saturating_sub(Span::new().days(370));
        self.contributions = self
            .database
            .day_progress(&contribution_start.to_string(), &self.today.to_string())?;
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
    #[error(transparent)]
    InvalidRoutineName(#[from] RoutineNameError),
    #[error("the active timezone has no IANA identifier")]
    UnnamedTimeZone,
    #[error("{0} is historical and read-only; press t to return to today")]
    ReadOnlyHistory(Date),
    #[error("{0} is an upcoming read-only preview; press t to return to today")]
    ReadOnlyFuture(Date),
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

    #[test]
    fn routines_settings_history_and_contributions_share_the_application_core() {
        let database = Database::open_in_memory(DataEnvironment::Test).unwrap();
        let timestamp: Timestamp = "2026-08-23T12:00:00Z".parse().unwrap();
        let mut application = HabitApplication::new(
            database,
            FixedClock::new(timestamp),
            FixedTimeZone::new(TimeZone::UTC),
        )
        .unwrap();

        application.create_daily_binary("read").unwrap();
        application.create_routine("morning").unwrap();
        let habit_id = application.habits()[0].habit_id;
        let routine_id = application.routines()[0].id;
        application
            .update_habit_settings(habit_id, "read ten pages", &[routine_id])
            .unwrap();

        assert_eq!(application.habits()[0].name, "read ten pages");
        assert_eq!(application.habits()[0].routines[0].name, "morning");
        assert_eq!(application.contributions().len(), 1);
        assert_eq!(application.contributions()[0].percentage(), 0);

        application
            .select_date(date(2026, 8, 22))
            .expect("history should be browsable");
        assert!(!application.is_viewing_today());
        assert!(application.habits().is_empty());
        assert!(matches!(
            application.create_daily_binary("must fail"),
            Err(ApplicationError::ReadOnlyHistory(_))
        ));
    }

    #[test]
    fn future_dates_preview_schedules_without_creating_activity() {
        let database = Database::open_in_memory(DataEnvironment::Test).unwrap();
        let timestamp: Timestamp = "2026-08-24T12:00:00Z".parse().unwrap();
        let mut application = HabitApplication::new(
            database,
            FixedClock::new(timestamp),
            FixedTimeZone::new(TimeZone::UTC),
        )
        .unwrap();
        application.create_daily_binary("read").unwrap();

        application.select_date(date(2026, 8, 25)).unwrap();

        assert!(application.is_viewing_future());
        assert!(application.habits().is_empty());
        assert_eq!(application.projected_habits()[0].name, "read");
        assert_eq!(application.contributions().len(), 1);
        assert!(matches!(
            application.create_daily_binary("must fail"),
            Err(ApplicationError::ReadOnlyFuture(_))
        ));
        assert!(matches!(
            application.create_routine("must also fail"),
            Err(ApplicationError::ReadOnlyFuture(_))
        ));
    }
}
