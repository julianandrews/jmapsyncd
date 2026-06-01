# jmapsyncd

JMAP ↔ Maildir sync daemon.

## Quick start

```bash
cargo install --path .

mkdir -p ~/.config/jmapsyncd
cat > ~/.config/jmapsyncd/config.toml << 'EOF'
[[accounts]]
name = "personal"
jmap_host = "api.fastmail.com"
jmap_user = "user@fastmail.com"
jmap_token = "your-api-token"

[accounts.mail]
path = "~/Mail/personal"
EOF

jmapsyncd sync      # one-shot sync
jmapsyncd daemon    # long-running daemon
```

## Configuration

Config file: `~/.config/jmapsyncd/config.toml` (set via `--config` or `JMAPSYNCD_CONFIG`).

```toml
# Global settings
db_dir = "~/.local/share/jmapsyncd"

[[accounts]]
name = "personal"
enabled = true
jmap_host = "api.fastmail.com"
jmap_user = "user@fastmail.com"

# Token — exactly one of:
jmap_token = ""                # inline
# jmap_token_file = "~/.config/jmapsyncd/tokens/personal"  # file path
# jmap_token_cmd = "get-token"                              # command

timeout_secs = 30

[accounts.mail]
path = "~/Mail/personal"
sync_mode = "mirror"         # "mirror" (pull-only) or "two_way"
subscribed_only = true       # only sync subscribed mailboxes
box_filter = ["INBOX"]       # override subscribed_only with globs

  [accounts.mail.tls]
  ca_file = "/etc/ssl/certs/ca-certificates.crt"
  # client_cert = "~/.config/jmapsyncd/cert.pem"
  # client_key  = "~/.config/jmapsyncd/key.pem"
  # fingerprint = "SHA256:..."

[[accounts.mail.box_mapping]]
remote = "Sent Items"
local  = "Sent"
```

All paths support `~`, `$VAR`, and `${VAR:-default}` expansion.

## Building

```bash
cargo build --release
./target/release/jmapsyncd --help
```
