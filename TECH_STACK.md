# ippo: Technical Stack

This document is the technical source of truth for ippo. It records the selected implementation stack, why each component fits the product, and the technical boundaries agents and contributors must preserve.

Read [`IPPO_CONCEPT.md`](./IPPO_CONCEPT.md) first. Product requirements take precedence over implementation convenience. This document explains how ippo is to be built; `IPPO_CONCEPT.md` explains what must be built and why.

## Stack summary

| Concern | Selected technology |
| --- | --- |
| Language | Rust, stable toolchain |
| Build and dependency management | Cargo |
| Terminal UI rendering | Ratatui |
| Terminal backend and input events | Crossterm |
| Date, time, and IANA timezone handling | Jiff |
| Local database | SQLite |
| Rust SQLite access | Rusqlite with bundled SQLite |
| Command-line interface | Clap with its derive API |
| Serialization and export | Serde and `serde_json` |
| Domain error types | Thiserror |
| Application-level error propagation | Anyhow, limited to executable boundaries |
| Multiline writing input | `tui-textarea`, introduced when writing habits are implemented |
| Automated testing | Rust's built-in test framework and Ratatui's `TestBackend`/buffers |
| Async runtime | None initially |

Dependency versions belong in `Cargo.toml` and `Cargo.lock`, not in this document. Use compatible stable releases, commit the lockfile because ippo is an application, and upgrade dependencies deliberately with tests.

## Rust

**Role:** The implementation language for the domain model, application logic, CLI, TUI, and persistence layer.

**Why it fits ippo:**

- Rust produces native executables suitable for a real terminal application.
- Strong enums and types fit ippo's distinct habit types, occurrence states, schedules, and progress rules.
- Explicit error handling helps protect durable local history from silent or partial failure.
- Ownership and concurrency guarantees reduce classes of state and memory errors in a long-running interactive application.
- The same core library can serve both the interactive TUI and scriptable CLI commands without duplicating business logic.
- Rust has a mature terminal-tool ecosystem and supports macOS, Linux, and Windows targets.

**Documentation:**

- [The Rust Programming Language](https://doc.rust-lang.org/book/)
- [Rust standard library](https://doc.rust-lang.org/std/)
- [Rust editions](https://doc.rust-lang.org/edition-guide/)

Use the stable Rust toolchain. Do not require nightly Rust without a documented, compelling need and explicit approval.

## Cargo

**Role:** Project structure, dependency resolution, builds, tests, benchmarks, and packaging.

**Why it fits ippo:**

- Cargo is Rust's standard build and package system.
- It provides a reproducible application dependency graph through `Cargo.lock`.
- It supports library and binary targets, allowing ippo's domain/application code to remain separate from the executable entry point.
- It provides standard commands for formatting, linting, testing, documentation, and release builds.
- It supports platform targets needed to distribute ippo to macOS, Linux, and Windows.

**Documentation:**

- [The Cargo Book](https://doc.rust-lang.org/cargo/)
- [Cargo build command](https://doc.rust-lang.org/cargo/commands/cargo-build.html)

## Ratatui

**Role:** Terminal layout, widgets, styled text, drawing buffers, and responsive dashboard rendering.

**Why it fits ippo:**

- Ratatui is designed for full-screen Rust terminal user interfaces.
- Its immediate-mode rendering model suits a dashboard whose status, habits, calendar, contribution graph, and focus state update together.
- Flexible layout constraints support ippo's wide, medium, and narrow terminal arrangements.
- Low-level widget and buffer APIs allow ippo to develop its own visual identity instead of inheriting a GUI-like component system.
- Its backend abstraction keeps presentation separate from terminal I/O.
- Its in-memory `TestBackend` and buffers allow deterministic UI tests at multiple terminal sizes.

**Documentation:**

- [Ratatui documentation and guides](https://ratatui.rs/)
- [Ratatui API documentation](https://docs.rs/ratatui/)
- [Ratatui `TestBackend`](https://docs.rs/ratatui/latest/ratatui/backend/struct.TestBackend.html)

Ratatui is a rendering library, not the application's domain architecture. Do not place scheduling, persistence, completion, XP, or streak rules inside rendering widgets.

## Crossterm

**Role:** Cross-platform terminal setup and restoration, raw mode, alternate screen, keyboard and mouse events, cursor control, and terminal resize events.

**Why it fits ippo:**

- Crossterm supports the major desktop platforms ippo targets, including Windows.
- It is Ratatui's default and most commonly used backend.
- It provides the event primitives needed for keyboard-first navigation and optional mouse interaction.
- Raw mode, alternate-screen handling, and resize events are the foundation of a responsive, native TUI.
- Choosing the conventional Ratatui backend makes examples, troubleshooting, and contributor onboarding easier.

**Documentation:**

- [Crossterm API documentation](https://docs.rs/crossterm/)
- [Ratatui backend guide](https://ratatui.rs/concepts/backends/)
- [Ratatui backend comparison](https://ratatui.rs/concepts/backends/comparison/)

Keep Ratatui and direct Crossterm dependencies on compatible Crossterm major versions. Multiple incompatible Crossterm versions can maintain separate event queues and raw-mode state, causing lost events or incorrect terminal restoration.

Terminal restoration must be treated as correctness-critical. Normal exit, handled errors, and panics should leave the user's terminal usable whenever technically possible.

## Jiff

**Role:** UTC instants, civil calendar dates, IANA time zones, daylight-saving-aware projection, parsing, formatting, and deterministic clock tests.

**Why it fits ippo:**

- Jiff distinguishes precise timestamps from civil dates and zone-aware datetimes, matching ippo's distinction between audit instants and calendar-day occurrences.
- It integrates with the IANA Time Zone Database and performs daylight-saving-aware calculations.
- It can discover the system timezone across Unix, macOS, and Windows while allowing explicit IANA zones.
- Its high-level primitives make invalid or ambiguous calendar operations harder to perform accidentally.
- It supports Serde where date/time values need explicit serialized representations.

**Documentation:**

- [Jiff API documentation](https://docs.rs/jiff/)
- [Jiff project documentation](https://github.com/BurntSushi/jiff)
- [Jiff timezone documentation](https://docs.rs/jiff/latest/jiff/tz/)

The date/time policy is:

- Record audit/event instants as UTC timestamps.
- Represent a habit occurrence by an ISO civil date (`YYYY-MM-DD`) in the timezone that scheduled that occurrence.
- Preserve the IANA timezone identifier used for the projection whenever timezone context is needed to interpret history.
- Treat a recorded historical occurrence's civil date as stable; travelling or changing the active timezone must not silently move it to another day.
- Apply timezone-setting changes to future day projection and scheduling, not by rewriting historical records.
- Obtain the current instant and active timezone through injectable abstractions. Domain and application code must not call the system clock or system timezone throughout the codebase.
- Detect day rollover while the TUI remains open and refresh today's occurrences without requiring a restart.

Jiff remains pre-1.0 at the time of selection, so keep its use behind ippo-owned clock/calendar boundaries and review release notes deliberately when upgrading.

## SQLite

**Role:** Durable local storage for habits, routines, schedules, dated occurrences, progress, writing entries, XP data, settings, and migrations.

**Why it fits ippo:**

- SQLite is an embedded database stored as a local file; it requires no server or account.
- Transactions support atomic updates and protect history from partial writes.
- Constraints, foreign keys, and indexes suit ippo's relational data and integrity requirements.
- It is portable, inspectable, easy to back up, and appropriate for a single-user local application.
- Its query capabilities support calendars, contribution aggregation, history, and later full-text search without introducing a backend service.

**Documentation:**

- [SQLite documentation](https://www.sqlite.org/docs.html)
- [SQLite transactions](https://www.sqlite.org/lang_transaction.html)
- [SQLite foreign-key support](https://www.sqlite.org/foreignkeys.html)

The database schema must use explicit, ordered migrations. Never silently rebuild or destructively replace a user's database to avoid writing a migration.

The first persistent habit slice establishes a durable separation between:

- Habit definitions, which hold the current identity and lifecycle of a habit.
- Schedule records, which state when a habit should produce occurrences.
- Dated occurrence snapshots, which preserve the scheduled civil date, relevant IANA timezone, habit name and type as they existed for that occurrence, and its completion state.

Completion updates must change the dated occurrence, never the habit definition. Daily occurrences are materialized transactionally and uniquely per habit and civil date so reopening ippo is idempotent. This separation is a historical-integrity invariant even as the exact schema expands for additional schedules and habit types.

Routine membership uses a many-to-many definition table. Dated occurrences snapshot both their habit display name and routine memberships, including routine names and ordering, so later settings changes do not relabel earlier history. Editing a habit may refresh the active day's snapshot, but it must not rewrite prior occurrence snapshots.

Daily schedules are reconciled through the current civil date when the application starts or detects a day rollover. This materializes missed scheduled days as incomplete occurrences, allowing calendar history and contribution aggregation to distinguish a real zero-completion day from an unscheduled day. Contribution percentages are aggregated from dated occurrence rows in SQLite and rendered from view-ready application data; rendering code does not calculate persistence semantics.

## Rusqlite with bundled SQLite

**Role:** Synchronous Rust access to SQLite.

**Why it fits ippo:**

- Rusqlite is a focused, ergonomic SQLite binding without requiring an asynchronous runtime.
- ippo performs short, local transactions against one user's database; synchronous access is simpler and sufficient.
- The `bundled` feature compiles and links a known SQLite version into the application, avoiding reliance on an old or missing system SQLite installation.
- Bundling improves consistency across macOS, Linux, and especially Windows distributions.
- Rusqlite exposes transactions, prepared statements, backup support, and SQLite configuration directly.

**Documentation:**

- [Rusqlite API documentation](https://docs.rs/rusqlite/)
- [Rusqlite project and bundled-SQLite guidance](https://github.com/rusqlite/rusqlite#usage)

Database operations belong behind repository interfaces or concrete storage modules. TUI widgets and CLI argument types must not issue SQL directly.

## Data-environment isolation

**Role:** Prevent development and automated testing from changing a user's real habits, writing, XP, streaks, or contribution history.

ippo must maintain three distinct classes of data environment:

| Environment | Purpose | Persistence |
| --- | --- | --- |
| Personal | Real day-to-day use | Persistent SQLite database in the platform-standard application-data location |
| Development | Manual feature and TUI testing | Separate persistent SQLite database selected as a development profile |
| Automated test | Unit and integration testing | Fresh in-memory or temporary on-disk database for each test |

These environments must never share a writable SQLite file. Isolation is enforced by path selection, database metadata, visible interface state, and test architecture rather than relying only on a developer remembering a command-line flag.

### Runtime selection

The application must support explicit data-environment selection. The intended controls are:

- A named profile selector. Debug builds support `personal` and `dev`; release builds support only `personal`.
- An `IPPO_PROFILE` environment variable for choosing a named profile.
- An explicit database-path override for experiments and tooling.
- An `IPPO_DATABASE` environment variable for the same low-level override.

The intended precedence from highest to lowest is:

```text
explicit database-path argument
IPPO_DATABASE
explicit profile argument
IPPO_PROFILE
personal default
```

The exact public flag spelling can be finalized with the wider CLI grammar, but the precedence and ability to select an isolated database are required.

The checked-in `.cargo/config.toml` sets `IPPO_PROFILE=dev` with `force = false`. Consequently, ordinary `cargo run` sessions started through Cargo use development data. A developer can intentionally switch to real personal data with `cargo run -- --profile personal`.

The `dev` profile is compiled only into debug builds. Release binaries—the artifacts distributed to users—do not advertise or accept it and default to personal data. The internal `development` database identity remains recognizable in release builds so a release executable can reject a development-tagged database instead of reclassifying or writing to it. To launch a release build through Cargo, use `IPPO_PROFILE=personal cargo run --release`; an installed executable runs outside Cargo's checked-in environment configuration.

### Database identity guard

Every persistent database must store an environment identity such as `personal` or `development` in application metadata. Before enabling writes, ippo must verify that the resolved runtime environment agrees with the database identity.

If development mode resolves to a database marked personal, or personal mode resolves to a database marked development, ippo must refuse to write and report the mismatch. Reclassifying a database must require a separate deliberate operation; normal startup must never rewrite its identity automatically.

### Visible development state

The TUI must make a non-personal environment obvious. Development mode should show a persistent label such as `[DEVELOPMENT]` with a distinct visual treatment. Diagnostics or help must expose the resolved profile, environment identity, and database path.

This protects manual testing sessions from being mistaken for real use and makes bug reports easier to reproduce.

### Development fixtures

Development tooling should be able to seed deterministic sample data that exercises:

- Every supported habit type.
- Multiple routines and schedules.
- Partial and complete days.
- Streaks of different lengths.
- Contribution history across months and years.
- Writing entries above and below their configured minimum.
- Paused and archived habits.
- Calendar and scheduling edge cases.

Seed and reset operations must identify their target environment clearly and refuse to operate on a database marked personal. Resetting development data is allowed only through an explicit operation; ordinary application startup must not replace it.

### Test databases

Automated tests must inject their database connection or path directly. They must not call the normal personal-profile resolver and must not rely on the developer's `IPPO_PROFILE` or `IPPO_DATABASE` values.

Use:

- Fresh in-memory SQLite databases for most repository and domain integration tests.
- Unique temporary on-disk databases for migrations, reopening, backup, locking, and filesystem behavior.
- A fresh migrated schema and explicit fixture data for every test that touches persistence.

Tests must never open the personal or persistent development database, even read-only, unless a narrowly scoped migration-compatibility procedure explicitly copies a user-provided database into a temporary location first. All migration and mutation testing then occurs against the copy.

### Controlled time

Domain and application logic must receive the current time or calendar date through an injectable clock abstraction rather than reading the system clock throughout the codebase.

Tests must be able to fix `today` deterministically for midnight, weekday schedules, month/year boundaries, leap years, daylight-saving transitions, streaks, and contribution calculations. A manual development date override may be provided, but it must be visibly indicated and must refuse to run against personal data.

### Privacy and filesystem behavior

- Personal databases and writing entries must never be stored inside the source repository by default.
- Development data stored inside a checkout must be covered by `.gitignore` before it is created.
- Temporary test data must be cleaned up by the test harness when practical.
- Database paths printed in diagnostics are local paths; diagnostic output must not include private writing or habit contents unless explicitly requested.

## Clap

**Role:** Parsing `ippo` arguments, options, and scriptable subcommands.

**Why it fits ippo:**

- Clap is a mature Rust CLI parser with generated help and validation.
- Its derive API provides typed subcommands with limited parsing boilerplate.
- It supports the intended dual interface: running `ippo` without a subcommand opens the TUI, while commands such as `ippo today` or `ippo done` call the same application layer non-interactively.
- Typed command structures are testable without involving terminal rendering.

**Documentation:**

- [Clap API documentation](https://docs.rs/clap/)
- [Clap derive tutorial](https://docs.rs/clap/latest/clap/_derive/_tutorial/)

Exact command names and flags remain product decisions. Clap is the selected parser, not permission to invent a large command surface prematurely.

## Serde and `serde_json`

**Role:** Explicit serialization for JSON backup, export, import, configuration, and test fixtures where appropriate.

**Why they fit ippo:**

- Serde is Rust's standard serialization framework and keeps serialization rules explicit and type-driven.
- `serde_json` provides interoperable, human-inspectable JSON for user-owned exports.
- Versioned export structures can remain separate from the SQLite schema and internal domain representation.
- The combination supports reliable round-trip tests for backup and restore behavior.

**Documentation:**

- [Serde documentation](https://serde.rs/)
- [`serde_json` API documentation](https://docs.rs/serde_json/)

Do not serialize internal database rows as the permanent public export format by accident. Export formats must be intentionally versioned so the application can evolve without stranding backups.

## Thiserror

**Role:** Typed errors in domain, application, and storage modules.

**Why it fits ippo:**

- It reduces boilerplate while preserving meaningful error variants and sources.
- Typed errors let callers distinguish validation, persistence, migration, and domain failures.
- It supports actionable user-facing messages without erasing diagnostic context.

**Documentation:**

- [Thiserror API documentation](https://docs.rs/thiserror/)

## Anyhow

**Role:** Adding context and reporting unexpected failures at executable boundaries such as `main`, startup, and top-level command dispatch.

**Why it fits ippo:**

- It makes top-level propagation concise while retaining error chains and contextual information.
- It is appropriate where callers do not need to exhaustively match error variants.
- Used alongside Thiserror, it keeps library-facing errors structured while making final reporting practical.

**Documentation:**

- [Anyhow API documentation](https://docs.rs/anyhow/)

Do not use `anyhow::Error` as the default domain or repository API. Domain behavior should remain explicit and testable through typed errors.

## `tui-textarea`

**Role:** Multiline terminal editing for writing/reflection habits when that feature is implemented.

**Why it fits ippo:**

- It integrates with Ratatui and Crossterm.
- It already supports multiline input, cursor movement, scrolling, selection, undo/redo, and customizable key handling.
- Reusing a focused editor component avoids turning the writing-habit feature into an accidental text-editor project.
- ippo can still add `$EDITOR` integration later for users who prefer their existing terminal editor.

**Documentation:**

- [`tui-textarea` API documentation](https://docs.rs/tui-textarea/)

Add this dependency only when writing input is implemented, and confirm that its Ratatui and Crossterm versions are compatible with the application's dependency graph at that time.

## Testing stack

**Role:** Verifying domain rules, migrations, persistence, CLI behavior, event handling, and responsive UI rendering.

**Why it fits ippo:**

- Rust includes unit, integration, and documentation testing through Cargo.
- Ratatui widgets render to deterministic buffers that can be asserted without a live terminal.
- `TestBackend` supports full-screen integration tests and simulated terminal sizes.
- Temporary SQLite databases can verify migrations and repository behavior without touching the user's real data.
- The data-environment contract ensures automated tests cannot resolve to personal or persistent development storage.

**Documentation:**

- [Testing in The Rust Programming Language](https://doc.rust-lang.org/book/ch11-00-testing.html)
- [Cargo test command](https://doc.rust-lang.org/cargo/commands/cargo-test.html)
- [Ratatui `TestBackend`](https://docs.rs/ratatui/latest/ratatui/backend/struct.TestBackend.html)

Critical tests should cover more than screenshots. Keep most scheduling, progress, contribution, streak, and XP logic as terminal-independent pure code with direct behavioral tests.

Persistence tests must follow the isolation requirements in [Data-environment isolation](#data-environment-isolation). A test that can fall back to the personal database is a correctness bug, even if CI normally prevents it.

## No async runtime initially

ippo will begin without Tokio or another asynchronous runtime.

**Why this fits ippo:**

- The core application is local and single-user.
- Crossterm's event loop and short synchronous SQLite transactions are sufficient for the initial product.
- Avoiding async reduces dependencies, state coordination, shutdown complexity, and the risk of blocking/async boundaries spreading through the architecture.
- An async runtime can be added later if a concrete feature demonstrates a need for it; hypothetical future synchronization is not enough reason to add one now.

This is an intentional technical decision, not an absolute prohibition. Any proposal to introduce async must identify the real blocking workload, the ownership and shutdown model, and why a simpler worker thread or synchronous operation is insufficient.

## Architectural shape

The initial codebase should keep interface code around a shared application core:

```text
ippo executable
├── CLI parsing and command dispatch
├── TUI event loop and rendering
├── application services / use cases
├── domain model and rules
└── SQLite repositories and migrations
```

The important dependency direction is inward:

- The domain model must not depend on Ratatui, Crossterm, Clap, or Rusqlite.
- Application use cases may depend on domain types and repository abstractions.
- SQLite modules implement persistence concerns without leaking SQL into the UI.
- Both the CLI and TUI invoke the same application behavior.
- Rendering reads view-ready state; it does not define business rules.

A single Cargo package with a library target and one `ippo` binary is sufficient initially. Do not introduce a multi-crate workspace until the codebase has a concrete boundary that benefits from it.

## Technical priorities

When technical tradeoffs arise, prefer:

1. Durable data and safe migrations over implementation shortcuts.
2. Simple synchronous control flow over speculative concurrency.
3. Domain logic independent of terminal and database frameworks.
4. Deterministic behavior that can be tested without a live terminal.
5. Cross-platform terminal behavior over platform-specific tricks.
6. A small dependency graph over convenience crates with marginal value.
7. Clear stable APIs over premature abstraction.
8. Enforced personal/development/test isolation over convenient implicit path selection.

## Decisions intentionally left open

This stack does not yet settle:

- The exact database schema and migration mechanism built around Rusqlite.
- Exact CLI grammar and keyboard bindings.
- Exact XP, level, completion-weighting, and streak formulas.
- Exact visual theme and responsive breakpoints.
- Packaging, release automation, and distribution channels.

Resolve these through explicit product and technical decisions supported by tests. Do not treat the selected stack as having decided them implicitly.
