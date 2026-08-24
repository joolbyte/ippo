# ippo

ippo is a fast, free, open-source, local-first habit tracker that genuinely lives in the terminal.

The project is in its foundation stage. Read [`IPPO_CONCEPT.md`](./IPPO_CONCEPT.md) for the product vision and [`TECH_STACK.md`](./TECH_STACK.md) for the technical decisions.

## Development

The stable Rust toolchain is required.

```sh
cargo run
```

Running through Cargo defaults to the isolated `dev` profile. The TUI identifies development mode visibly and exits with `q`, `Esc`, or `Ctrl+C`.

The current habit workflow is available in the TUI:

- Press `n`, enter a name, and press `Enter` to create a daily binary habit.
- Press `r` to create a routine.
- Move through today's habits with `j`/`k` or the arrow keys.
- Press `e` on a habit to rename it, change its routine memberships, or archive it. Use `Tab` to change fields and `Space` to toggle routines. Archiving requires a separate confirmation, removes the habit from today and future dates, and preserves its past history.
- Press `Space` to toggle the selected habit's completion for today. Completed habits move below all unchecked habits.
- Use `h`/`l` or the left/right arrows to browse dates, `[`/`]` to move by month, and `t` to return to today. Historical dates are read-only, while future dates are non-persisted read-only previews.
- Press `Tab` to cycle between focused Today, Calendar, and Contributions views; this keeps history available in compact terminals.
- The calendar and contribution graph are computed from persisted dated occurrences; contribution intensity reflects each day's completion percentage.
- Press `q` or `Esc` to quit. Habits and dated completion state survive restart.

To intentionally run against your personal profile while developing:

```sh
cargo run -- --profile personal
```

This command writes to the same personal database used by an installed copy of ippo, so use it only for real activity. Return to development data by running ordinary `cargo run` again.

Release builds do not expose or accept the `dev` profile. A distributed `ippo` executable defaults to the personal profile; development data exists only for debug builds made from a source checkout. If you launch a release build through Cargo, use `IPPO_PROFILE=personal cargo run --release` because the checked-in Cargo configuration otherwise supplies the debug-only profile.

Inspect the active environment without opening the TUI:

```sh
cargo run -- doctor
cargo run -- doctor --json
```

Run the verification suite:

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
```

Automated tests use isolated in-memory or temporary databases. They never use personal or persistent development data.

## Repository status

The application provides a persistent binary-habit workflow with unlimited routines, multi-routine membership, safe name and routine editing, non-destructive archival, read-only calendar history and future previews, contribution aggregation, completion toggling, day rollover, and responsive TUI states. Richer habit types, schedule editing, writing, XP, levels, and streaks remain future work.
