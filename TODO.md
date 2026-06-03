# Build Plan

Dependency graph:

```
Task 1 (scaffold)
  ├── Task 2 (DB)
  └── Task 3 (sync engine + JMAP client)
       └── Task 4 (Phase 1 binary)
  Task 3
       └── Task 5 (Phase 2 push)
```

Tasks 2 and 3 are independent and can run in parallel after Task 1.

---

- [x] **Task 1 — Scaffold + Config + CLI + Logging**
  - Populate `Cargo.toml` with all dependencies
  - Create module stubs (`db`, `jmap`, `sync`)
  - Implement `config.rs`: all config structs, serde deserialization, path expansion, token resolution
  - Implement `logging.rs`: `env_logger` init with level precedence
  - Implement CLI with `clap` derive (options + subcommands)
  - Re-export public API from `lib.rs`
  - Tests: config edge cases, CLI parsing, token validation

- [x] **Task 2 — Database layer**
  - `db/mod.rs`: connection, WAL mode, integrity check, migrations
  - `db/models.rs`: row types + CRUD for `mailboxes`, `emails`, `email_mailboxes`
  - Tests: in-memory SQLite round-trips for all CRUD

- [ ] **Task 3 — Core sync engine + JMAP client**
  - `jmap/mod.rs`: connection helpers (build `jmap_client::Client` from account config — token resolution, TLS setup)
  - `sync/mailbox.rs`: mailbox list sync (create/rename/delete directories)
  - `sync/email.rs`: three-way email diff, keyword↔flag mapping (use `maildir` crate directly for Maildir ops)
  - `sync/mod.rs`: primary mailbox selection, top-level `sync_account()`
  - Use `debug!` logging for every individual message that changed locally or on the remote (new, updated flags, deleted)
  - Tests: sync cycles with `wiremock`-mocked HTTP + in-memory DB + temp Maildir (no client trait or mock client — use `jmap_client::Client` pointed at a `wiremock::MockServer`)

- [ ] **Task 4 — Phase 1 binary (daemon loop + SSE)**
  - Wire `sync` command (one-shot)
  - Wire `daemon`/default mode with SSE reconnect loop + polling timer
  - `--dry-run` support
  - Integration test with `wiremock` mock JMAP server

- [ ] **Task 5 — Phase 2 (file watcher + push)**
  - `sync/push.rs`: upload new, push flag changes, delete server copies (use `maildir` crate for file ops, `notify` for watching)
  - Conflict resolution (server-wins)
  - Tests: watcher in temp dirs, bidirectional sync with wiremock
