# Build Plan

Dependency graph:

```
Task 1 (scaffold)
  ├── Task 2 (DB)
  └── Task 3 (JMAP trait + mock)
       └── Task 4 (sync engine)
            └── Task 5 (Phase 1 binary)
 Task 4
       └── Task 6 (Phase 2 push)
```

Tasks 2 and 3 are independent and can run in parallel after Task 1.

---

- [x] **Task 1 — Scaffold + Config + CLI + Logging**
  - Populate `Cargo.toml` with all dependencies
  - Create module stubs (`db`, `jmap`, `maildir`, `sync`)
  - Implement `config.rs`: all config structs, serde deserialization, path expansion, token resolution
  - Implement `logging.rs`: `env_logger` init with level precedence
  - Implement CLI with `clap` derive (options + subcommands)
  - Re-export public API from `lib.rs`
  - Tests: config edge cases, CLI parsing, token validation

- [x] **Task 2 — Database layer**
  - `db/mod.rs`: connection, WAL mode, integrity check, migrations
  - `db/models.rs`: row types + CRUD for `mailboxes`, `emails`, `email_mailboxes`
  - Tests: in-memory SQLite round-trips for all CRUD

- [ ] **Task 3 — JMAP abstraction layer**
  - `jmap/mod.rs`: `JmapClient` trait with all needed methods
  - `JmapMockClient`: in-memory stub for testing
  - `JmapLiveClient`: wraps `jmap-client` crate, token resolution, TLS config
  - Tests: mock client scenarios

- [ ] **Task 4 — Core sync engine**
  - `sync/mailbox.rs`: mailbox list sync (create/rename/delete directories)
  - `sync/email.rs`: three-way email diff, keyword↔flag mapping (use `maildir` crate directly for Maildir ops)
  - `sync/mod.rs`: primary mailbox selection, top-level `sync_account()`
  - Use `debug!` logging for every individual message that changed locally or on the remote (new, updated flags, deleted)
  - Tests: full sync cycles with `JmapMockClient` + in-memory DB + temp Maildir

- [ ] **Task 5 — Phase 1 binary (daemon loop + SSE)**
  - Wire `sync` command (one-shot)
  - Wire `daemon`/default mode with SSE reconnect loop + polling timer
  - `--dry-run` support
  - Integration test with `wiremock` mock JMAP server

- [ ] **Task 6 — Phase 2 (file watcher + push)**
  - `maildir/watcher.rs`: `notify` watcher, event debounce, `is_dirty` marking (use `maildir` crate for file ops)
  - `sync/push.rs`: upload new, push flag changes, delete server copies
  - Conflict resolution (server-wins)
  - Tests: watcher in temp dirs, bidirectional sync with mock
