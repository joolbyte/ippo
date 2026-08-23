# Agent Instructions

## Mandatory first steps

Before doing anything in this repository—including planning, answering questions, reviewing code, editing files, running commands, or proposing architecture—read [`IPPO_CONCEPT.md`](./IPPO_CONCEPT.md) in full.

Treat `IPPO_CONCEPT.md` as the authoritative product source of truth. Keep all implementation, design, naming, scope, and UX decisions aligned with it.

Before doing any technical work—including architecture, implementation, dependency selection, database design, terminal behavior, testing, packaging, or technical review—also read [`TECH_STACK.md`](./TECH_STACK.md) in full.

Treat `TECH_STACK.md` as the authoritative technical source of truth. Technical work must satisfy both documents; if they appear to conflict, stop and surface the conflict instead of silently choosing one.

## Working rules

- Inspect the repository's current state before claiming what is implemented or working.
- Distinguish established decisions from unresolved choices listed in `IPPO_CONCEPT.md` and `TECH_STACK.md`.
- Do not expand ippo into a goal planner, task manager, general notes app, social platform, or cloud-dependent service unless the user explicitly revises the concept.
- Preserve the local-first, offline-capable, keyboard-first, free, unlimited, and terminal-native principles.
- Protect historical integrity and user privacy, especially for habit history and writing entries.
- Never use the personal or persistent development database for automated tests. Use a fresh in-memory or temporary database, inject its path/connection directly, and do not allow tests to fall back to normal profile resolution.
- Use the development data environment for manual testing. Never run seed, reset, migration experiments, simulated-date sessions, or feature testing against personal data. Follow the isolation contract in `TECH_STACK.md`.
- Preserve unrelated user changes and do not commit or push unless explicitly requested.
- For implementation tasks, complete the requested work and verify it in proportion to its risk; do not stop at a plan when safe implementation is possible.
- If a request conflicts with `IPPO_CONCEPT.md` or `TECH_STACK.md`, clearly identify the conflict and ask whether the relevant source-of-truth document should be revised before proceeding.
- When an accepted product decision changes, update `IPPO_CONCEPT.md` as part of the same work. When an accepted technical decision changes, update `TECH_STACK.md`. Keep both documents consistent so future agents receive current guidance.
