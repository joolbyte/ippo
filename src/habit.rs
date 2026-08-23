use thiserror::Error;

pub const MAX_HABIT_NAME_CHARS: usize = 80;

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
}
