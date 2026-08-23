# ippo: Product Concept and Vision

This document is the product source of truth for ippo. It is written for coding agents and human contributors who need to understand the project quickly, preserve its identity, and make implementation decisions that serve the intended product.

## One-sentence definition

ippo is a fast, free, open-source, local-first habit tracker that genuinely lives in the terminal.

## Product promise

ippo should make recording and reviewing habits feel as immediate as running a shell command. It gives users a polished, keyboard-first terminal dashboard for answering three questions:

1. What habits should I do today?
2. How am I doing today?
3. How consistent have I been over time?

The user owns the application and its data. There are no accounts, subscriptions, cloud requirements, paywalls, artificial limits, or features locked behind progression.

## Origin and identity

ippo was inspired by the visual appeal of init.Habits: monospace presentation, command-like headings, compact progress indicators, routines, calendars, streaks, and a GitHub-style contribution graph.

ippo is not intended to be a clone. Its fundamental distinction is that it is a real terminal application rather than a graphical interface styled to look like one. The terminal is not a visual theme; it is the application's native environment.

The name `ippo` comes from the Japanese word for "one step" and expresses the product's philosophy: meaningful change is built through small, repeated actions.

## Core principles

### 1. Actually terminal-native

The primary experience is an interactive terminal user interface (TUI), launched directly from a terminal. It should feel at home alongside high-quality terminal tools rather than imitate a browser or desktop application.

Keyboard control is primary. Mouse support may supplement it, but must never be required.

Where useful, the same data and actions should also be available through scriptable CLI commands. For example:

```sh
ippo
ippo today
ippo done read
ippo log water 1
ippo journal --date 2026-08-23
```

Exact command syntax is not yet fixed; examples communicate the intended experience.

### 2. Local-first and user-owned

All core functionality must work offline. Habit definitions, schedules, completions, measurements, and writing entries are stored locally, with SQLite as the intended durable store.

Users should be able to inspect, back up, import, and export their data. The core product must not depend on authentication, a hosted backend, telemetry, or synchronization.

Optional synchronization could be considered much later, but it must never compromise the fully local experience or become necessary to use the application.

### 3. Free and unlimited

ippo is free and open source. Users may create unlimited habits and unlimited routines. There are no premium tiers and no artificial usage restrictions.

### 4. Fast enough to disappear

Opening ippo, finding today's habits, recording progress, and leaving should be nearly frictionless. Common interactions should require very few keystrokes and should respond immediately.

### 5. Focused on habits

ippo is a habit tracker, not a general life planner, project manager, task manager, or goal-setting system.

The application does not need a separate goals feature. Users may decide their broader goals outside ippo; ippo helps them perform and understand the repeated actions that support those goals.

### 6. Motivating without coercion

Progress visualization and light gamification should make consistency satisfying without punishing the user or turning the product into a grind.

XP and levels are celebratory indicators of continued use and completed habits. They never unlock functionality, impose limits, or provide competitive advantages.

## Core domain model

The precise schema may evolve, but the product should preserve these concepts.

### Habits

A habit is a repeated action scheduled for particular days or intervals. A habit belongs to zero or more routines and produces dated occurrences that can be completed or progressed.

Expected habit types include:

- **Binary:** complete or incomplete, such as "brush teeth."
- **Count or quantity:** progress toward a number, such as "drink 8 glasses of water" or "read 20 pages."
- **Duration:** progress measured in time, such as "meditate for 10 minutes."
- **Writing or reflection:** a locally stored text response, such as "summarize my day" or "write about my feelings."

Habit types should share a coherent history and completion model while retaining the information specific to each type.

### Routines

A routine is a named group of habits, such as `morning`, `deep work`, `personal`, or `evening`. Routines organize the daily experience; they are not a paywalled or limited resource.

A habit may need to appear in more than one organizational context, so implementations should avoid assuming that a habit can only ever belong to one routine unless the product explicitly decides otherwise.

### Scheduled occurrences

The application should reason about a habit as a definition and each scheduled day as a dated occurrence. This distinction is important for preserving history when a habit is edited, paused, archived, rescheduled, or otherwise changed later.

Historical records should remain truthful. Editing today's habit definition must not silently rewrite what the user actually did in the past.

### Completions and progress

An occurrence may be incomplete, partially complete, or complete depending on its habit type.

Examples:

- A binary habit is either incomplete or complete.
- A quantity habit may be `4 / 8` and therefore 50% complete.
- A writing habit becomes complete when its entry is saved and satisfies any configured minimum.

The application's daily completion percentage should be based on the progress of the habits scheduled for that day. The exact weighting policy must be explicit, consistent, and testable.

## Writing and lightweight journaling

Writing is a first-class habit type, not a separate general-purpose notes application.

A user can define habits such as:

- "Summarize my day" with a minimum of 100 characters.
- "How am I feeling?" with a minimum of 20 characters.
- "Record one thing I learned" with a short free-text response.

Writing habits should support:

- Single-line or multiline input.
- An optional minimum character count.
- An optional maximum character count, although a minimum is generally more useful.
- Saving one dated entry for each scheduled occurrence.
- Editing a saved entry without losing its original date association.
- Browsing previous entries by habit and by date.
- Local storage alongside all other ippo data.
- Inclusion in backup and export.

A writing occurrence counts as complete when a saved entry meets its configured minimum. If there is no minimum, a non-empty saved entry is sufficient.

Longer entries may eventually be editable through either an in-app multiline editor or the user's `$EDITOR`. This is a terminal-native enhancement, not a requirement for the earliest usable version.

The product boundary is important: writing remains attached to recurring habits and dated occurrences. ippo should not drift into becoming a general wiki, knowledge base, or unstructured notes manager.

## Primary interface

The default `ippo` screen should feel like a complete personal dashboard. On a sufficiently wide terminal it should present four areas:

- **Status:** today's completion percentage, streak information, level, XP, and related summary information.
- **Today:** the scheduled habits grouped into routines, including current progress.
- **Calendar:** a navigable view of daily history and the current place in time.
- **Contributions:** a longer-term consistency heatmap.

Each area has one clear job:

| Area | Question answered |
| --- | --- |
| Status | How am I doing today? |
| Today | What do I need to do? |
| Calendar | What happened on a particular day? |
| Contributions | What does my consistency look like over time? |

The dashboard may be dense, but it should remain legible and responsive. Layouts should adapt progressively rather than require one fixed terminal size:

- Wide terminals can show multiple panes side by side.
- Medium terminals can move secondary panes below the main content.
- Narrow terminals can stack panes or expose them as focused views.

The interface should use tasteful terminal-native visual language: monospace text, clear borders and separators, concise status text, progress bars, heatmap cells, and restrained color. It should develop its own identity instead of reproducing init.Habits exactly.

ippo's visual identity is Japanese-inspired without becoming ornamental or themed like a novelty interface. The primary wordmark is `ippo 一歩`, pairing the Latin name with the Japanese characters for “one step.” Its dark palette draws from sumi ink, warm washi paper, vermilion red, moss green, indigo, and restrained gold. Vermilion anchors borders, selection, and emphasis; moss communicates completion; indigo supports secondary structure. This direction should remain calm, compact, and highly legible in real terminals.

## Contribution graph semantics

The contribution graph is a central product feature. It resembles GitHub's calendar heatmap but represents habit completion quality rather than a binary completed/not-completed state.

Each day's intensity should reflect that day's aggregate completion percentage. For example, a day at 25% should be visibly dimmer than a day at 75%, and a fully completed day should use the strongest intensity.

Missing, unscheduled, future, and zero-completion days must be represented deliberately rather than accidentally conflated. The exact visual scale can evolve, but percentage-based intensity is a core requirement.

## Calendar and history

Users should be able to return to a past date and understand what was scheduled, what was completed, the progress recorded, and any writing saved that day.

History should support reflection rather than shame. Missed days are data, not failures that require punitive messaging.

Habits should generally be archived instead of destructively deleted when historical data exists.

## Levels, XP, and streaks

### Levels and XP

Completing habits awards XP and allows the user to level up. This system is intentionally a fun gimmick and a visible indicator of long-term engagement.

Nothing is locked behind a level. Leveling must not control access to habit counts, routine counts, themes, analytics, writing, export, or any other capability.

XP rules should be understandable and resistant to obvious accidental inflation, but they do not need to simulate an economy or support competitive play.

### Streaks

Streaks can make continuity visible, but their semantics must be fair for scheduled habits. An unscheduled day should not automatically break a habit streak. Pauses, archives, schedule changes, and partial completion require explicit, predictable handling.

The product should avoid aggressive loss framing. A streak is feedback, not a threat.

## Data integrity and privacy

The user's habit history and writing may be personal or sensitive. Implementations must therefore prioritize:

- Durable local persistence.
- Explicit schema migrations.
- Transactional updates where partial writes could corrupt state.
- Stable identifiers rather than relying on display names.
- Clear date and timezone behavior.
- Non-destructive editing and archival.
- Reliable backup and export.
- No transmission of user data without explicit user action.
- Hard separation between personal data, development data, and automated-test data.

Dates in a habit tracker are calendar concepts, not merely timestamps. Scheduling and daily aggregation must behave predictably across midnight, daylight-saving changes, travel, and timezone changes. The final policy should be documented and tested before those edge cases are considered complete.

ippo records audit instants in UTC while associating habit occurrences with stable civil dates and their relevant IANA timezone context. Changing timezones must not silently move or rewrite historical occurrences; it changes how future days are projected and scheduled. The concrete clock and timezone implementation is recorded in [`TECH_STACK.md`](./TECH_STACK.md).

Development activity and automated tests must never silently write to a user's personal database. Manual development uses a visibly identified, persistent development environment; automated tests use disposable isolated databases. Because XP, levels, streaks, and contribution history derive from stored activity, this separation is part of product data integrity rather than merely a contributor convenience.

## Intended experience

An ordinary session should feel roughly like this:

1. The user runs `ippo`.
2. The dashboard appears immediately.
3. Today's routines and habits are already selected from their schedules.
4. The user moves with familiar keyboard controls and records completions, quantities, durations, or writing.
5. Status, XP, the calendar, and contribution intensity update immediately.
6. Everything is persisted locally without an account or network connection.
7. The user quits and returns to the shell.

The app should also reward deeper exploration: selecting a date reveals that day's history, selecting a habit reveals its record over time, and selecting a writing habit allows previous entries to be reread.

## Scope boundaries

The following are deliberately outside the core vision unless this document is explicitly revised:

- General goal planning or outcome tracking.
- Project and task management.
- Social feeds, leaderboards, or competitive progression.
- Required public profiles.
- Required accounts or cloud synchronization.
- Paid tiers, feature gates, or habit/routine limits.
- General-purpose note taking unrelated to a habit occurrence.
- AI analysis of private writing by default.
- A GUI that merely imitates a terminal.

## Product priorities

When tradeoffs arise, prefer:

1. Correct and durable local data over visual novelty.
2. Fast completion logging over elaborate configuration.
3. A coherent habit-focused product over feature breadth.
4. Keyboard accessibility over mouse-dependent interaction.
5. Honest history over convenient but destructive rewriting.
6. Clear, calm motivation over pressure or punishment.
7. Cross-platform terminal behavior over terminal-specific tricks that break elsewhere.

## Delivery sequence

The implementation plan may change as the repository develops, but agents should generally establish capabilities in this order:

1. Durable domain model and local persistence.
2. Habit and routine creation, editing, scheduling, and archival.
3. Today's occurrences and reliable completion/progress tracking.
4. A usable keyboard-first TUI.
5. Calendar and historical inspection.
6. Percentage-based contribution graph.
7. Writing habits and entry history.
8. XP, levels, streak refinements, themes, and other polish.
9. Scriptable CLI commands, import/export, packaging, and distribution as appropriate.

This ordering is guidance, not permission to leave requested work half-finished. When implementing a scoped change, complete and verify it in proportion to its risk.

## Decision status

The following are established product decisions:

- ippo is a real terminal application.
- It is free, open source, local-first, offline-capable, and unlimited.
- It is centered on habits and routines, not goals.
- The default experience is a responsive dashboard.
- The visual identity uses the `ippo 一歩` wordmark and a restrained Japanese-inspired sumi, washi, vermilion, moss, indigo, and gold palette.
- The contribution graph uses completion percentage for intensity.
- XP and levels exist only for motivation and never unlock features.
- Writing/reflection is a habit type with a configurable minimum character count and locally stored dated entries.
- SQLite is the intended local persistence layer.
- The implementation uses Rust, Ratatui, Crossterm, and the supporting components recorded in [`TECH_STACK.md`](./TECH_STACK.md).
- Personal use, manual development, and automated tests use separate data environments and must never share a writable SQLite database.

The following remain implementation decisions and should not be assumed without repository evidence or an explicit decision:

- Exact database schema.
- Exact CLI grammar and keyboard bindings.
- Exact XP formula, level curve, and streak formula.
- Packaging and distribution channels.
- Whether optional synchronization will ever be built.

When an implementation choice is unresolved, agents should preserve the concept above, inspect the current repository, and choose the smallest durable approach that does not unnecessarily foreclose future work.

For settled implementation technologies and technical boundaries, refer to [`TECH_STACK.md`](./TECH_STACK.md).
