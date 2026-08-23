use jiff::{Timestamp, civil::Date, tz::TimeZone};

/// Supplies the current time to application behavior.
///
/// Keeping time behind this trait prevents scheduling and streak logic from
/// reading the process clock throughout the codebase.
pub trait Clock {
    fn now(&self) -> Timestamp;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        Timestamp::now()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FixedClock {
    now: Timestamp,
}

impl FixedClock {
    pub const fn new(now: Timestamp) -> Self {
        Self { now }
    }
}

impl Clock for FixedClock {
    fn now(&self) -> Timestamp {
        self.now
    }
}

/// Supplies the IANA time zone used to project an instant onto a calendar day.
pub trait TimeZoneSource {
    fn time_zone(&self) -> TimeZone;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemTimeZone;

impl TimeZoneSource for SystemTimeZone {
    fn time_zone(&self) -> TimeZone {
        TimeZone::system()
    }
}

#[derive(Debug, Clone)]
pub struct FixedTimeZone {
    time_zone: TimeZone,
}

impl FixedTimeZone {
    pub const fn new(time_zone: TimeZone) -> Self {
        Self { time_zone }
    }
}

impl TimeZoneSource for FixedTimeZone {
    fn time_zone(&self) -> TimeZone {
        self.time_zone.clone()
    }
}

/// Projects the current instant into the active local calendar day.
pub fn today(clock: &impl Clock, time_zone: &impl TimeZoneSource) -> Date {
    clock.now().to_zoned(time_zone.time_zone()).date()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_clock_is_deterministic() {
        let expected = Timestamp::UNIX_EPOCH;
        let clock = FixedClock::new(expected);

        assert_eq!(clock.now(), expected);
        assert_eq!(clock.now(), expected);
    }

    #[test]
    fn calendar_day_is_projected_through_injected_timezone() {
        let clock = FixedClock::new(Timestamp::UNIX_EPOCH);
        let time_zone = FixedTimeZone::new(TimeZone::UTC);

        assert_eq!(today(&clock, &time_zone), jiff::civil::date(1970, 1, 1));
    }
}
