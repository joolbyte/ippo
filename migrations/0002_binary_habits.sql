CREATE TABLE habits (
    id              INTEGER PRIMARY KEY NOT NULL,
    name            TEXT NOT NULL CHECK (length(trim(name)) BETWEEN 1 AND 80),
    habit_type      TEXT NOT NULL CHECK (habit_type IN ('binary')),
    created_at_utc  TEXT NOT NULL,
    archived_at_utc TEXT
) STRICT;

CREATE TABLE habit_schedules (
    id                    INTEGER PRIMARY KEY NOT NULL,
    habit_id              INTEGER NOT NULL REFERENCES habits(id),
    schedule_kind         TEXT NOT NULL CHECK (schedule_kind IN ('daily')),
    starts_on             TEXT NOT NULL,
    ends_on               TEXT,
    created_timezone      TEXT NOT NULL,
    created_at_utc        TEXT NOT NULL,
    CHECK (ends_on IS NULL OR ends_on >= starts_on)
) STRICT;

CREATE INDEX habit_schedules_active_idx
    ON habit_schedules (schedule_kind, starts_on, ends_on, habit_id);

CREATE TABLE habit_occurrences (
    id               INTEGER PRIMARY KEY NOT NULL,
    schedule_id      INTEGER NOT NULL REFERENCES habit_schedules(id),
    habit_id         INTEGER NOT NULL REFERENCES habits(id),
    scheduled_date   TEXT NOT NULL,
    timezone         TEXT NOT NULL,
    habit_name       TEXT NOT NULL,
    habit_type       TEXT NOT NULL CHECK (habit_type IN ('binary')),
    completed        INTEGER NOT NULL DEFAULT 0 CHECK (completed IN (0, 1)),
    completed_at_utc TEXT,
    created_at_utc   TEXT NOT NULL,
    updated_at_utc   TEXT NOT NULL,
    UNIQUE (habit_id, scheduled_date),
    CHECK (
        (completed = 0 AND completed_at_utc IS NULL)
        OR (completed = 1 AND completed_at_utc IS NOT NULL)
    )
) STRICT;

CREATE INDEX habit_occurrences_date_idx
    ON habit_occurrences (scheduled_date, id);
