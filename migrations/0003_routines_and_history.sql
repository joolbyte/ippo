CREATE TABLE routines (
    id              INTEGER PRIMARY KEY NOT NULL,
    name            TEXT NOT NULL COLLATE NOCASE UNIQUE
                    CHECK (length(trim(name)) BETWEEN 1 AND 40),
    created_at_utc  TEXT NOT NULL,
    archived_at_utc TEXT
) STRICT;

CREATE TABLE habit_routines (
    habit_id      INTEGER NOT NULL REFERENCES habits(id),
    routine_id    INTEGER NOT NULL REFERENCES routines(id),
    position      INTEGER NOT NULL DEFAULT 0 CHECK (position >= 0),
    created_at_utc TEXT NOT NULL,
    PRIMARY KEY (habit_id, routine_id)
) STRICT;

CREATE INDEX habit_routines_routine_idx
    ON habit_routines (routine_id, position, habit_id);

CREATE TABLE habit_occurrence_routines (
    occurrence_id INTEGER NOT NULL REFERENCES habit_occurrences(id),
    routine_id    INTEGER NOT NULL,
    routine_name  TEXT NOT NULL,
    position      INTEGER NOT NULL DEFAULT 0 CHECK (position >= 0),
    PRIMARY KEY (occurrence_id, routine_id)
) STRICT;

CREATE INDEX habit_occurrence_routines_occurrence_idx
    ON habit_occurrence_routines (occurrence_id, position, routine_id);
