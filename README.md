# ippo

ippo is a fast, free, open-source, local-first habit tracker that genuinely lives in the terminal.

The project is in its foundation stage. Read [`IPPO_CONCEPT.md`](./IPPO_CONCEPT.md) for the product vision and [`TECH_STACK.md`](./TECH_STACK.md) for the technical decisions.

## Development

The stable Rust toolchain is required.

```sh
cargo run
```

Running through Cargo defaults to the isolated `dev` profile. The TUI identifies development mode visibly and exits with `q`, `Esc`, or `Ctrl+C`.

The first real habit loop is available in the TUI:

- Press `n`, enter a name, and press `Enter` to create a daily binary habit.
- Move through today's habits with `j`/`k` or the arrow keys.
- Press `Space` to toggle the selected habit's completion for today. Completed habits move below all unchecked habits.
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

The application provides the safe runtime foundation plus its first persistent vertical slice: daily binary-habit creation, today's dated occurrences, completion toggling, day rollover, and responsive empty/active TUI states. Routines, richer habit types, editing, archival, history, contribution aggregation, and gamification remain future work.
