# JMAP sync client

Bidirectional JMAP ↔ Maildir sync daemon.

## Dependencies

### Core

- `serde` for CLI/config parsing
- `clap` (with derive) for CLI
- `toml` for config
- `anyhow` for error handling
- `dirs` for config/data paths
- `rusqlite` (with `features = ["bundled"]`)
- `serde_rusqlite` — deserialize rusqlite rows into structs via `#[derive(Deserialize)]`
- `jmap-client` for JMAP connection
- `notify` for watching for maildir changes
- `maildir` - for maildir operations
- `globset` - for mailbox pattern matching (`box_filter`)
- `shellexpand` - for path expansion (`~`, `$VAR` in config paths)
- `log` + `env_logger` - for logging (levels, `RUST_LOG`, colored output)
- `uuid` - for generating local_id values (v4, simple and standard)

### Optional (as needed)

- `thiserror` (if creating a meaningful library layer)
- `mail-parser` (if we end up parsing mail)
- `jmap-tools` (dunno - seems like a high quality library, we might need it)

### Dev

- `tempfile` - scratch directories for filesystem tests
- `wiremock` - mock JMAP HTTP responses

## Module structure

```
jmapsyncd/
├── Cargo.toml
└── src/
    ├── lib.rs            # Re-exports public API
    ├── args.rs           # CLI definition (clap derive)
    ├── bin/
    │   └── jmapsyncd.rs  # CLI entry point, daemon loop, top-level orchestration
    ├── config.rs          # Config structs, deserialize, expand, resolved config
    ├── logging.rs         # env_logger init
    ├── db/
    │   ├── mod.rs         # Database struct (open, WAL, integrity, migrate) + CRUD methods
    │   └── models.rs      # Row types (pure data structs with #[derive(Deserialize)])
    ├── jmap/
    │   └── mod.rs         # JMAP connection helpers (build Client from account config)
    └── sync/
        ├── mod.rs         # Sync orchestration, three-way diff
        ├── mailbox.rs     # Mailbox list sync
        └── email.rs       # Email download/upload logic
```

Single crate with `lib.rs` keeps compilation fast and makes the library
testable. Phase 2 modules (`sync/push.rs`) are
added later as new files. The `maildir` crate is used directly
for Maildir file operations.

## Configuration

### Precedence (highest to lowest)

1. CLI arguments (from `clap`)
2. Environment variables
3. Config file at `{config_dir}/jmapsyncd/config.toml`, where `config_dir` is
   the platform config directory from the `dirs` crate
   (e.g. `~/.config/jmapsyncd/config.toml` on Linux)

### Config file format

```toml
db_dir = "~/.local/share/jmapsyncd"
log_level = "info"

[[accounts]]
name = "personal"

jmap_host = "api.fastmail.com"
jmap_user = "user@fastmail.com"

# Token — exactly one of these three variants
jmap_token = "..."                        # inline (least secure)
# jmap_token_file = "~/.config/jmapsyncd/tokens/personal"  # file
# jmap_token_cmd = "oauth2token get user@fastmail.com"     # command

[accounts.mail]
path = "~/Mail/personal"
sync_mode = "mirror"     # "mirror" = pull-only, "two-way" = bidirectional
subscribed_only = true   # only sync mailboxes with isSubscribed=true
box_filter = ["INBOX", "Sent*"]  # glob patterns (overrides subscribed_only)

  [accounts.mail.tls]
  ca_file = "/etc/ssl/certs/ca-certificates.crt"
  # client_cert = "~/.config/jmapsyncd/cert.pem"
  # client_key = "~/.config/jmapsyncd/key.pem"
  # fingerprint = "SHA256:..."

[[accounts.mail.box_mapping]]
remote = "Sent Items"
local = "Sent"

# Future: [accounts.contacts]
# Future: [accounts.calendars]
```

### Config types (Rust)

Two structs serve different roles:

- **`ConfigFile`** (private) — maps directly to the TOML file; owns `Deserialize`.
- **`Config`** (public) — the final merged config after precedence resolution
  (CLI overrides → file → defaults). Returned by `Config::load()`.

```rust
// --- Merged config used by the rest of the code ---

#[derive(Debug)]
pub struct Config {
    pub db_dir: PathBuf,
    pub accounts: Vec<Account>,
}

impl Config {
    pub fn load(path: Option<&Path>, overrides: &Overrides) -> Result<Self> { ... }
}

#[derive(Debug, Default)]
pub struct Overrides {
    pub db_dir: Option<PathBuf>,
}

// --- TOML file deserialization (private) ---

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigFile {
    #[serde(default, deserialize_with = "helpers::expand_opt_path")]
    db_dir: Option<PathBuf>,
    #[serde(default)]
    accounts: Vec<Account>,
}
```

Per-account:
```rust
#[derive(Debug, Deserialize)]
pub struct Account {
    pub name: String,
    #[serde(default = "helpers::default_true")]
    pub enabled: bool,
    pub jmap_host: String,
    pub jmap_user: String,
    #[serde(flatten)]
    pub token: TokenSource,            // exactly one via untagged serde enum
    #[serde(default = "helpers::default_timeout_secs")]
    pub timeout_secs: u64,             // default 30
    pub mail: Option<MailConfig>,
}

// Token is an untagged enum — serde tries variants in order
// and picks the first that matches. "Exactly one" is enforced
// by deserialization failure when no variant matches.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum TokenSource {
    Inline { jmap_token: String },
    File {
        #[serde(deserialize_with = "helpers::expand_path")]
        jmap_token_file: PathBuf,
    },
    Cmd { jmap_token_cmd: String },
}
```

Mail settings:
```rust
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MailConfig {
    #[serde(deserialize_with = "helpers::expand_path")]
    pub path: PathBuf,
    #[serde(default)]
    pub sync_mode: SyncMode,
    #[serde(default = "helpers::default_true")]
    pub subscribed_only: bool,
    pub box_filter: Option<Vec<String>>,
    pub tls: Option<TlsConfig>,
    #[serde(default)]
    pub box_mapping: Vec<BoxMapping>,
}
```

Leaf types:
```rust
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TlsConfig {
    #[serde(default, deserialize_with = "helpers::expand_opt_path")]
    pub ca_file: Option<PathBuf>,
    #[serde(default, deserialize_with = "helpers::expand_opt_path")]
    pub client_cert: Option<PathBuf>,
    #[serde(default, deserialize_with = "helpers::expand_opt_path")]
    pub client_key: Option<PathBuf>,
    pub fingerprint: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BoxMapping {
    pub remote: String,
    pub local: String,
}

#[derive(Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SyncMode {
    #[default]
    Mirror,
    TwoWay,
}
```

`#[serde(deny_unknown_fields)]` is used on `ConfigFile`, `MailConfig`, and
`TlsConfig` to catch typos at parse time. It is intentionally **not** on
`Account` because `#[serde(flatten)]` + `#[serde(untagged)]` are incompatible
with `deny_unknown_fields` (serde can't know upfront which fields the
untagged variant will consume).

### Path expansion

All path fields (`db_dir`, `jmap_token_file`, `path`, `ca_file`, `client_cert`,
`client_key`) support shell-style expansion via the `shellexpand` crate:

- `~` → expanded to `$HOME` (e.g. `~/Mail` → `/home/user/Mail`)
- `$VAR` / `${VAR}` → expanded from environment variables
- `${VAR:-default}` → fallback default if unset

Expansion happens **at parse time** via per-field serde helpers
(`helpers::expand_path` for required paths,
`helpers::expand_opt_path` for optional paths). Every `PathBuf` field
in the config structs is annotated with
`#[serde(deserialize_with = "helpers::expand_path")]` or
`#[serde(default, deserialize_with = "helpers::expand_opt_path")]`.
The rest of the code always works with absolute, expanded paths and
there is no post-deserialization `expand_paths` step to maintain.

### CLI flags

Built with `clap` derive in `src/args.rs`. All account-level config belongs in
`config.toml` — the CLI only controls runtime behavior and basic overrides.

```
jmapsyncd [OPTIONS] [COMMAND]

OPTIONS:
    -c, --config <PATH>        Config file path [env: JMAPSYNCD_CONFIG]
    --db-dir <PATH>            Override data directory [env: JMAPSYNCD_DB_DIR]
    --log-level <LEVEL>        Log level (trace|debug|info|warn|error) [env: JMAPSYNCD_LOG_LEVEL]
    -n, --dry-run              Preview changes without applying anything
    -V, --version              Print version

COMMANDS:
    sync    [ACCOUNT]   One-shot sync run, then exit. Omit account to sync all.
    daemon              Start the long-running daemon (default — runs if no command is given)
```

Default (`jmapsyncd` with no command) starts the daemon. Use `jmapsyncd sync`
for initial setup, scripting, or `--dry-run` testing.

### Environment variables

| Variable | Overrides | Description |
|---|---|---|
| `JMAPSYNCD_CONFIG` | Config file path | Custom config location |
| `JMAPSYNCD_DB_DIR` | `db_dir` | Data directory root |
| `JMAPSYNCD_LOG_LEVEL` | `log_level` | Log level (trace/debug/info/warn/error) |
| `RUST_LOG` | — | Standard log env var; alternative to `JMAPSYNCD_LOG_LEVEL` |

Precedence for all config-overridable values (`--db-dir`, etc.):
1. CLI flag wins over everything
2. Dedicated env var (`JMAPSYNCD_DB_DIR`) wins over config file value
3. Config file value wins over hardcoded default

`log_level` is **not** in the config file — it is CLI/env only, with
`RUST_LOG` as a fallback. See [Logging](#logging).

## Logging

Initialized on startup via `env_logger`. Effective level:
1. `--log-level` CLI flag
2. `JMAPSYNCD_LOG_LEVEL` env var (handled by clap)
3. `RUST_LOG` env var (handled by `env_logger::Builder::from_default_env()`)
4. Hardcoded `info` default

There is no `log_level` in the config file — it is purely a runtime/debugging
concern, not a sync configuration concern.

| Level | What goes there |
|---|---|
| `error!` | Irrecoverable failures — DB corruption, auth failure, sync abort. User must act. |
| `warn!` | Recoverable failures — skipped a message, rate limited. Sync continues. |
| `info!` | High-level lifecycle — "sync started", "account X: 47 new, 3 deleted", "config reloaded". One line per sync per account. |
| `debug!` | Per-message operations — "downloaded msg abc-123", "updated flags on def-456". |
| `trace!` | Protocol-level — raw JMAP request/response bodies, filesystem events. Too noisy for anything but development. |

Account context is added manually:
```rust
info!("[{}] sync completed: {} new, {} updated", account.name, new, updated);
```

Tokens, passwords, and full message bodies must never appear in logs at
`info` or below. `debug` and `trace` may include message IDs and truncated
content (first 200 chars of subject) where useful.

## State management

### Design principle

**SQLite is canonical state, files are canonical content.**
- Standard Maildir filenames (`{timestamp}.{uniquifier}:2,{flags}` — handled
  by the `maildir` crate). No custom UID infixes like mbsync's `,U=`.
- SQLite stores the mapping between local files and JMAP server IDs, plus
  sync state tokens and keywords.

### Database storage

`db_dir` defaults to the platform data dir (e.g. `~/.local/share/jmapsyncd`
on Linux, via the `dirs` crate). Each account has one subdirectory; each
data type has its own flat database file:

```
~/.local/share/jmapsyncd/
├── personal/
│   ├── mail.sqlite
│   ├── mail.sqlite-wal
│   ├── mail.sqlite-shm
│   └── contacts.sqlite       ← future
└── work/
    ├── mail.sqlite
    ├── mail.sqlite-wal
    ├── mail.sqlite-shm
    └── contacts.sqlite       ← future
```

Full path: `{db_dir}/{account_name}/{type}.sqlite`.

### Schema (mail)

These tables live in `{db_dir}/{account_name}/mail.sqlite`. Future data types
(contacts, calendars) get their own databases.

```sql
-- Mailboxes: local UUID as PK; jmap_id is NULL until created server-side
CREATE TABLE mailboxes (
    id          TEXT PRIMARY KEY,  -- local UUID, assigned on first sight
    jmap_id     TEXT UNIQUE,       -- JMAP server ID, NULL if not yet created on server
    name        TEXT NOT NULL,
    parent_id   TEXT REFERENCES mailboxes(id),  -- local UUID (stable even if parent unsynced)
    role        TEXT,              -- "inbox", "sent", "trash", "drafts", "archive", "junk"
    sort_order  INTEGER,
    path        TEXT NOT NULL,     -- local Maildir path (e.g. "INBOX", "INBOX.Sent")
    jmap_state  TEXT               -- last known MailboxList state token
);
CREATE INDEX idx_mailboxes_parent_id ON mailboxes(parent_id);
CREATE INDEX idx_mailboxes_path ON mailboxes(path);

-- Emails: one row per message, one Maildir file per row
CREATE TABLE emails (
    id                TEXT PRIMARY KEY,       -- UUID, assigned on first sight
    jmap_id           TEXT UNIQUE,            -- Server's JMAP Email ID, NULL if not yet uploaded
    message_id        TEXT,                   -- Message-Id header (RFC 5322), for recovery
    file_path         TEXT UNIQUE NOT NULL,   -- Absolute path to the email file
    primary_mailbox   TEXT NOT NULL           -- FK → mailboxes(id), determines file_path location
                  REFERENCES mailboxes(id),
    keywords          TEXT,                   -- JSON object of JMAP keywords
    jmap_state        TEXT,                   -- Last known Email state token
    size              INTEGER,                -- File size in bytes
    last_sync         INTEGER,                -- Unix timestamp of last successful sync
    is_dirty          BOOLEAN DEFAULT 1       -- Local changes need to be pushed
);
CREATE INDEX idx_emails_message_id ON emails(message_id);
CREATE INDEX idx_emails_jmap_id ON emails(jmap_id);

-- Join table: all mailbox memberships (JMAP allows one email in many mailboxes)
CREATE TABLE email_mailboxes (
    email_id  TEXT REFERENCES emails(id) ON DELETE CASCADE,
    mailbox_id      TEXT REFERENCES mailboxes(id) ON DELETE CASCADE,
    is_primary      BOOLEAN DEFAULT 0,    -- true for the single primary mailbox
    PRIMARY KEY (email_id, mailbox_id)
);
CREATE INDEX idx_em_mailbox ON email_mailboxes(mailbox_id);
```

### Database safety

- WAL mode (`PRAGMA journal_mode=WAL`) for crash resilience
- `PRAGMA integrity_check` on every startup
- `PRAGMA synchronous=NORMAL` for good performance without risking corruption
- Periodic out-of-band backups via `VACUUM INTO '~/backups/jmapsyncd/personal/mail.sqlite'`

### Recovery from database loss

A full recovery is a rare event (corruption, accidental deletion). It is
**slow but reliable** — no file content is ever modified, no X-Headers injected.

**Pass 1 — Message-Id matching (≥95% of messages)**
1. Scan all local Maildir files, extract `Message-Id` header from each (first ~4KB of file)
2. JMAP `Email/query` + `Email/get` to fetch `[id, messageId]` for all server messages
3. Match local ↔ server by `Message-Id`. Rebuild `id ↔ jmap_id` mapping.

**Pass 2 — Content hash matching (remaining ≤5%)**
1. For local files still unmatched, compute SHA-256 (not stored in DB — computed on the fly)
2. Download raw blobs for all unmatched server messages (JMAP `Download/get`)
3. Compute SHA-256 of downloaded blobs, match against local hashes. Rebuild mapping.

**Pass 3 — Truly unmatched**
- Local file, no match on server → upload as new (mark dirty)
- Server message, no match locally → download as new local file

**Duplicate prevention:**
- Pass 2 content hashing guarantees zero false positives
- The bandwidth cost of downloading unmatched server blobs is negligible

## Sync model

### Three-way sync model

Track three states between sync runs:
- **Server state** — what the JMAP server reports
- **Local state** — what's in the Maildir
- **Last-synced state** — what the DB says was last agreed

On each sync:
1. Wait for trigger: SSE `StateChange` notification, or polling timer fallback
2. Fetch server changes (via `Email/changes` using last-known state token)
3. Scan local Maildir for changes (via filesystem `notify` events + periodic full scan)
4. Diff both against DB state
5. Propagate changes in the direction they occurred

### Server change detection (SSE + polling)

Primary: JMAP EventSource (SSE) via `jmap-client`'s `event_source()` method.
The daemon holds a persistent HTTP connection and receives
`PushNotification::StateChange` events with affected account IDs and new state
tokens.

```rust
// reconnect loop with resume support
let mut last_event_id = None;
loop {
    let mut stream = client
        .event_source(types, false, 30.into(), last_event_id.as_deref())
        .await?;
    while let Some(event) = stream.next().await {
        match event? {
            PushNotification::StateChange(changes) => {
                last_event_id = changes.id().map(String::from);
                // queue a sync for each changed account
            }
        }
    }
    // stream dropped — reconnect after brief delay
    tokio::time::sleep(Duration::from_secs(1)).await;
}
```

Fallback: periodic polling timer (e.g. every 5 minutes) runs `Email/changes`
independently. This catches changes missed during SSE downtime and handles
servers that don't support push.

The sync operation is identical regardless of trigger — it always calls
`Email/changes` / `Mailbox/changes` with the last-known state token.

### Primary mailbox selection

Since JMAP allows an email to belong to multiple mailboxes but Maildir requires
a single physical file location, every email has exactly **one primary mailbox**
that determines where its file lives on disk. The DB records all memberships
in `email_mailboxes`; the filesystem only reflects the primary.

Selection rules, applied when an email is first seen or its membership changes:

1. **Role-based priority** — pick the mailbox with the highest-priority role:
   `inbox` > `sent` > `drafts` > `archive` > `trash` > `junk` > no role
2. **Lowest sortOrder** — if multiple mailboxes have the same priority
3. **Alphabetically by name** — tiebreaker for deterministic behavior

When the primary mailbox changes (user moves the message, or the current
primary is deleted), the daemon updates `is_primary` in the join table and
moves the file to the new mailbox's path.

## Phased implementation

### Phase 1 — Pull-only (server → local)

Goal: initial sync and mirror-mode daemon. Local Maildir is a read-only mirror
of the server.

- Discover JMAP mailboxes, create matching Maildir directories
- Download all emails, write to Maildir files, populate DB
- Monitor server changes via JMAP EventSource (SSE), with periodic polling
  fallback. On `StateChange` notification, fetch diffs via `Email/changes`.
- Download new/modified/deleted messages on change
- Sync server flags down to Maildir `:2,S` suffix
- DB rows set `is_dirty = 0` always (no local changes tracked yet)

Local file behavior (configurable, default = mirror):
| Setting | Deleted locally | New file locally |
|---|---|---|
| `mirror` (default) | Re-download from server | Delete local file, log warning |
| `leave` | Mark DB row clean, let it stand | Ignore local file |

- No local file watching via `notify` yet
- No uploads to server

### Phase 2 — Bidirectional (add local push)

Goal: full two-way sync. Local changes propagate to server.

- Enable `notify` watcher on Maildir directories
- Detect: new files, deleted files, file renames (flag changes)
- Mark affected DB rows `is_dirty = 1`
- Upload new messages via `Email/set` create
- Push flag changes via `Email/set` update (keywords)
- Handle server-side deletions propagating locally
- Handle locally deleted files removing server copies
- Conflict resolution (server-wins by default, configurable)

### Schema compatibility

The DB schema is the same across both phases. `is_dirty` is the only behavioral
toggle:
- Phase 1: always 0, never read
- Phase 2: set by file watcher, consumed by upload logic

## Testing

### Test placement

**Unit tests** — inline `#[cfg(test)] mod tests` in the source file containing
the code they test. Every module with non-trivial logic should have them.
Examples:

- Config deserialization edge cases (missing fields, unknown fields, expansion)
- Config merge/precedence (CLI override > file > default)
- DB query correctness (CRUD round-trips via in-memory SQLite)
- Three-way diff logic
- Primary mailbox selection rules
- Keyword↔flag mapping (JMAP keywords to Maildir flags and back)
- Token deserialization (exactly-one enforced by serde)

The pattern:

```rust
// src/config.rs
fn parse_flags(input: &str) -> Result<Flags> { ... }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_valid_flags() { ... }
    #[test]
    fn rejects_invalid_flags() { ... }
}
```

**Integration tests** — `tests/` directory, one file per major subsystem.
Examples:

- End-to-end sync with mocked JMAP server
- Config file loading and validation
- Daemon startup and shutdown sequence

### Testing JMAP interactions

`jmap-client` does not support injecting a mock HTTP client. Sync tests use
`wiremock` to simulate the JMAP server at the HTTP level. A `wiremock::MockServer`
is started in the test, and the real `jmap_client::Client` is pointed at it.

No trait or mock client is defined. The sync engine takes `&jmap_client::Client`
directly. This keeps the abstraction layer thin and avoids maintaining a
parallel test stub that can drift from the real HTTP behaviour.

### Per-test infrastructure

| Component | Tool | Pattern |
|---|---|---|---|
| Temp directories | `tempfile::tempdir()` | Dropped on test exit, no cleanup needed |
| SQLite database | `Database::open_in_memory()` | Run migrations, CRUD operations, drop |
| JMAP mock | `wiremock` | Start `MockServer`, configure routes matching JMAP request structure, point real `jmap_client::Client` at the mock |
| `notify` events | `tempfile` + real `notify::recommended_watcher()` | Create files in a temp dir, wait via `recv_timeout` |

`notify` tests use real filesystem operations on `tempfile`-created directories.
There is no mock watcher. Standard pattern:

```rust
let tmp = tempfile::tempdir().unwrap();
let (tx, rx) = std::sync::mpsc::channel();
let mut watcher = notify::recommended_watcher(tx).unwrap();
watcher.watch(tmp.path(), RecursiveMode::Recursive).unwrap();

// Trigger a change
File::create(tmp.path().join("new/test:2,")).unwrap();

// Wait for the async event (2s timeout for CI safety)
let event = rx.recv_timeout(Duration::from_secs(2)).unwrap();
assert!(matches!(event, Ok(Event { kind: EventKind::Create(_), .. })));
```

The `PollWatcher` backend is available as a CI-safe alternative if `inotify`
has issues in containers.
