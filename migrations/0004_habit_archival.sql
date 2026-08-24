ALTER TABLE habit_occurrences
ADD COLUMN excluded_at_utc TEXT;

ALTER TABLE habit_occurrences
ADD COLUMN exclusion_reason TEXT
CHECK (
    (excluded_at_utc IS NULL AND exclusion_reason IS NULL)
    OR (
        excluded_at_utc IS NOT NULL
        AND exclusion_reason IN ('habit_archived')
    )
);
