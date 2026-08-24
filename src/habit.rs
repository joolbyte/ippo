use jiff::civil::Date;
use thiserror::Error;

pub const MAX_HABIT_NAME_CHARS: usize = 80;
pub const MAX_ROUTINE_NAME_CHARS: usize = 40;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HabitName(String);

impl HabitName {
    pub fn parse(value: &str) -> Result<Self, HabitNameError> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(HabitNameError::Empty);
        }
        if trimmed.chars().count() > MAX_HABIT_NAME_CHARS {
            return Err(HabitNameError::TooLong {
                maximum: MAX_HABIT_NAME_CHARS,
            });
        }
        if trimmed.chars().any(char::is_control) {
            return Err(HabitNameError::ControlCharacter);
        }

        Ok(Self(trimmed.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TodayHabit {
    pub occurrence_id: i64,
    pub habit_id: i64,
    pub name: String,
    pub completed: bool,
    pub routines: Vec<Routine>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Routine {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DayProgress {
    pub date: Date,
    pub scheduled: usize,
    pub completed: usize,
}

impl DayProgress {
    pub fn percentage(&self) -> u16 {
        self.completed
            .saturating_mul(100)
            .checked_div(self.scheduled)
            .unwrap_or(0) as u16
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutineName(String);

impl RoutineName {
    pub fn parse(value: &str) -> Result<Self, RoutineNameError> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(RoutineNameError::Empty);
        }
        if trimmed.chars().count() > MAX_ROUTINE_NAME_CHARS {
            return Err(RoutineNameError::TooLong {
                maximum: MAX_ROUTINE_NAME_CHARS,
            });
        }
        if trimmed.chars().any(char::is_control) {
            return Err(RoutineNameError::ControlCharacter);
        }

        Ok(Self(trimmed.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RoutineNameError {
    #[error("routine name cannot be empty")]
    Empty,
    #[error("routine name cannot exceed {maximum} characters")]
    TooLong { maximum: usize },
    #[error("routine name cannot contain control characters")]
    ControlCharacter,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum HabitNameError {
    #[error("habit name cannot be empty")]
    Empty,
    #[error("habit name cannot exceed {maximum} characters")]
    TooLong { maximum: usize },
    #[error("habit name cannot contain control characters")]
    ControlCharacter,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn habit_name_is_trimmed_and_validated() {
        assert_eq!(HabitName::parse("  read  ").unwrap().as_str(), "read");
        assert_eq!(HabitName::parse("   ").unwrap_err(), HabitNameError::Empty);
        assert!(matches!(
            HabitName::parse(&"a".repeat(MAX_HABIT_NAME_CHARS + 1)),
            Err(HabitNameError::TooLong { .. })
        ));
    }

    #[test]
    fn routine_name_is_trimmed_and_validated() {
        assert_eq!(
            RoutineName::parse("  morning  ").unwrap().as_str(),
            "morning"
        );
        assert_eq!(
            RoutineName::parse("   ").unwrap_err(),
            RoutineNameError::Empty
        );
        assert!(matches!(
            RoutineName::parse(&"a".repeat(MAX_ROUTINE_NAME_CHARS + 1)),
            Err(RoutineNameError::TooLong { .. })
        ));
    }

    #[test]
    fn day_progress_preserves_percentage_intensity() {
        let progress = DayProgress {
            date: Date::new(2026, 8, 24).unwrap(),
            scheduled: 8,
            completed: 3,
        };

        assert_eq!(progress.percentage(), 37);
        assert_eq!(
            DayProgress {
                date: progress.date,
                scheduled: 0,
                completed: 0,
            }
            .percentage(),
            0
        );
    }
}
