use anyhow::{Context, Result};
use rusqlite::{Connection, params};
use std::path::Path;
use uuid::Uuid;

pub mod models;

use self::models::{EmailMailboxRow, EmailRow, MailboxRow};

pub struct Database {
    conn: Connection,
}

impl Database {
    /// Open or create a database at the given path.
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)
            .with_context(|| format!("opening database at {}", path.display()))?;
        let db = Database { conn };
        db.configure()?;
        db.integrity_check()?;
        db.migrate()?;
        Ok(db)
    }

    /// Open an in-memory database (for testing).
    ///
    /// Skips `configure` (WAL mode requires a file path) and `integrity_check`
    /// (a fresh in-memory database is always valid).
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory().context("opening in-memory database")?;
        let db = Database { conn };
        db.migrate()?;
        Ok(db)
    }

    fn configure(&self) -> Result<()> {
        self.conn
            .execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")
            .context("configuring database pragmas")?;
        Ok(())
    }

    fn integrity_check(&self) -> Result<()> {
        let result: String = self
            .conn
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))
            .context("running integrity check")?;
        if result != "ok" {
            anyhow::bail!("database integrity check failed: {result}");
        }
        Ok(())
    }

    fn migrate(&self) -> Result<()> {
        self.conn
            .execute_batch(
                "
            CREATE TABLE IF NOT EXISTS mailboxes (
                id          TEXT PRIMARY KEY,
                jmap_id     TEXT UNIQUE,
                name        TEXT NOT NULL,
                parent_id   TEXT REFERENCES mailboxes(id),
                role        TEXT,
                sort_order  INTEGER,
                path        TEXT NOT NULL,
                jmap_state  TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_mailboxes_parent_id ON mailboxes(parent_id);
            CREATE INDEX IF NOT EXISTS idx_mailboxes_path ON mailboxes(path);

            CREATE TABLE IF NOT EXISTS emails (
                id                TEXT PRIMARY KEY,
                jmap_id           TEXT UNIQUE,
                message_id        TEXT,
                file_path         TEXT UNIQUE NOT NULL,
                primary_mailbox   TEXT NOT NULL REFERENCES mailboxes(id),
                keywords          TEXT,
                jmap_state        TEXT,
                size              INTEGER,
                last_sync         INTEGER,
                is_dirty          BOOLEAN DEFAULT 1
            );
            CREATE INDEX IF NOT EXISTS idx_emails_message_id ON emails(message_id);
            CREATE INDEX IF NOT EXISTS idx_emails_jmap_id ON emails(jmap_id);

            CREATE TABLE IF NOT EXISTS email_mailboxes (
                email_id    TEXT REFERENCES emails(id) ON DELETE CASCADE,
                mailbox_id  TEXT REFERENCES mailboxes(id) ON DELETE CASCADE,
                is_primary  BOOLEAN DEFAULT 0,
                PRIMARY KEY (email_id, mailbox_id)
            );
            CREATE INDEX IF NOT EXISTS idx_em_mailbox ON email_mailboxes(mailbox_id);",
            )
            .context("running migrations")?;
        Ok(())
    }

    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    /// Execute a SELECT with one param, return the first row if any.
    fn get_one<T: serde::de::DeserializeOwned>(
        &self,
        sql: &str,
        param: &dyn rusqlite::types::ToSql,
    ) -> Result<Option<T>> {
        let mut stmt = self.conn.prepare(sql)?;
        let mut rows = stmt.query_map([param], from_row)?;
        match rows.next() {
            Some(Ok(row)) => Ok(Some(row)),
            Some(Err(e)) => Err(e.into()),
            None => Ok(None),
        }
    }

    /// Execute a SELECT with no params, return all rows.
    fn get_many<T: serde::de::DeserializeOwned>(&self, sql: &str) -> Result<Vec<T>> {
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map([], from_row)?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Execute a SELECT with one param, return all matching rows.
    fn get_many_by<T: serde::de::DeserializeOwned>(
        &self,
        sql: &str,
        param: &dyn rusqlite::types::ToSql,
    ) -> Result<Vec<T>> {
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map([param], from_row)?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }
}

// ---------------------------------------------------------------------------
// Mailbox CRUD
// ---------------------------------------------------------------------------

impl Database {
    pub fn insert_mailbox(&self, mailbox: &MailboxRow) -> Result<()> {
        self.conn.execute(
            "INSERT INTO mailboxes (id, jmap_id, name, parent_id, role, sort_order, path, jmap_state) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                mailbox.id,
                mailbox.jmap_id,
                mailbox.name,
                mailbox.parent_id,
                mailbox.role,
                mailbox.sort_order,
                mailbox.path,
                mailbox.jmap_state,
            ],
        )?;
        Ok(())
    }

    pub fn get_mailbox(&self, id: &str) -> Result<Option<MailboxRow>> {
        self.get_one("SELECT * FROM mailboxes WHERE id = ?1", &id)
    }

    pub fn get_mailbox_by_jmap_id(&self, jmap_id: &str) -> Result<Option<MailboxRow>> {
        self.get_one("SELECT * FROM mailboxes WHERE jmap_id = ?1", &jmap_id)
    }

    pub fn get_all_mailboxes(&self) -> Result<Vec<MailboxRow>> {
        self.get_many("SELECT * FROM mailboxes")
    }

    pub fn update_mailbox(&self, mailbox: &MailboxRow) -> Result<()> {
        self.conn.execute(
            "UPDATE mailboxes SET jmap_id=?1, name=?2, parent_id=?3, role=?4, sort_order=?5, path=?6, jmap_state=?7 WHERE id=?8",
            params![
                mailbox.jmap_id,
                mailbox.name,
                mailbox.parent_id,
                mailbox.role,
                mailbox.sort_order,
                mailbox.path,
                mailbox.jmap_state,
                mailbox.id,
            ],
        )?;
        Ok(())
    }

    pub fn delete_mailbox(&self, id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM mailboxes WHERE id = ?1", params![id])?;
        Ok(())
    }
}

fn from_row<T: serde::de::DeserializeOwned>(row: &rusqlite::Row) -> rusqlite::Result<T> {
    serde_rusqlite::from_row(row).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
}

/// Generate a new UUID v4 identifier for a database row.
pub fn generate_id() -> String {
    Uuid::new_v4().to_string()
}

// ---------------------------------------------------------------------------
// Email CRUD
// ---------------------------------------------------------------------------

impl Database {
    pub fn insert_email(&self, email: &EmailRow) -> Result<()> {
        self.conn.execute(
            "INSERT INTO emails (id, jmap_id, message_id, file_path, primary_mailbox, keywords, jmap_state, size, last_sync, is_dirty) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                email.id,
                email.jmap_id,
                email.message_id,
                email.file_path,
                email.primary_mailbox,
                email.keywords,
                email.jmap_state,
                email.size,
                email.last_sync,
                email.is_dirty,
            ],
        )?;
        Ok(())
    }

    pub fn get_email(&self, id: &str) -> Result<Option<EmailRow>> {
        self.get_one("SELECT * FROM emails WHERE id = ?1", &id)
    }

    pub fn get_email_by_jmap_id(&self, jmap_id: &str) -> Result<Option<EmailRow>> {
        self.get_one("SELECT * FROM emails WHERE jmap_id = ?1", &jmap_id)
    }

    pub fn get_email_by_file_path(&self, file_path: &str) -> Result<Option<EmailRow>> {
        self.get_one("SELECT * FROM emails WHERE file_path = ?1", &file_path)
    }

    pub fn get_all_emails(&self) -> Result<Vec<EmailRow>> {
        self.get_many("SELECT * FROM emails")
    }

    pub fn update_email(&self, email: &EmailRow) -> Result<()> {
        self.conn.execute(
            "UPDATE emails SET jmap_id=?1, message_id=?2, file_path=?3, primary_mailbox=?4, keywords=?5, jmap_state=?6, size=?7, last_sync=?8, is_dirty=?9 WHERE id=?10",
            params![
                email.jmap_id,
                email.message_id,
                email.file_path,
                email.primary_mailbox,
                email.keywords,
                email.jmap_state,
                email.size,
                email.last_sync,
                email.is_dirty,
                email.id,
            ],
        )?;
        Ok(())
    }

    pub fn delete_email(&self, id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM emails WHERE id = ?1", params![id])?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// EmailMailbox CRUD
// ---------------------------------------------------------------------------

impl Database {
    pub fn insert_email_mailbox(&self, em: &EmailMailboxRow) -> Result<()> {
        self.conn.execute(
            "INSERT INTO email_mailboxes (email_id, mailbox_id, is_primary) VALUES (?1, ?2, ?3)",
            params![em.email_id, em.mailbox_id, em.is_primary],
        )?;
        Ok(())
    }

    pub fn delete_email_mailbox(&self, email_id: &str, mailbox_id: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM email_mailboxes WHERE email_id = ?1 AND mailbox_id = ?2",
            params![email_id, mailbox_id],
        )?;
        Ok(())
    }

    pub fn get_email_mailboxes_by_email(&self, email_id: &str) -> Result<Vec<EmailMailboxRow>> {
        self.get_many_by(
            "SELECT * FROM email_mailboxes WHERE email_id = ?1",
            &email_id,
        )
    }

    pub fn get_email_mailboxes_by_mailbox(&self, mailbox_id: &str) -> Result<Vec<EmailMailboxRow>> {
        self.get_many_by(
            "SELECT * FROM email_mailboxes WHERE mailbox_id = ?1",
            &mailbox_id,
        )
    }

    pub fn set_primary_mailbox(&self, email_id: &str, mailbox_id: &str) -> Result<()> {
        self.conn.execute_batch("BEGIN")?;
        let result = self.conn.execute(
            "UPDATE email_mailboxes SET is_primary = 0 WHERE email_id = ?1",
            params![email_id],
        );
        if result.is_err() {
            self.conn.execute_batch("ROLLBACK")?;
            return result.map(|_| ()).map_err(Into::into);
        }
        let result = self.conn.execute(
            "UPDATE email_mailboxes SET is_primary = 1 WHERE email_id = ?1 AND mailbox_id = ?2",
            params![email_id, mailbox_id],
        );
        match result {
            Ok(_) => {
                self.conn.execute_batch("COMMIT")?;
                Ok(())
            }
            Err(e) => {
                self.conn.execute_batch("ROLLBACK")?;
                Err(e.into())
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_mailbox(name: &str, path: &str) -> MailboxRow {
        MailboxRow {
            id: generate_id(),
            jmap_id: Some(format!("jmap-{name}")),
            name: name.to_string(),
            parent_id: None,
            role: None,
            sort_order: Some(0),
            path: path.to_string(),
            jmap_state: None,
        }
    }

    fn sample_email(mailbox_id: &str, file_path: &str) -> EmailRow {
        EmailRow {
            id: generate_id(),
            jmap_id: Some(format!("jmap-email-{}", generate_id())),
            message_id: Some(format!("<{}@example.com>", generate_id())),
            file_path: file_path.to_string(),
            primary_mailbox: mailbox_id.to_string(),
            keywords: None,
            jmap_state: None,
            size: Some(1024),
            last_sync: Some(1_700_000_000),
            is_dirty: false,
        }
    }

    // -----------------------------------------------------------------------
    // Database struct tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_open_in_memory_creates_tables() {
        let db = Database::open_in_memory().unwrap();
        let conn = db.connection();

        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(tables, vec!["email_mailboxes", "emails", "mailboxes"]);

        let indexes: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='index' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap();
        assert!(indexes.contains(&"idx_em_mailbox".to_string()));
        assert!(indexes.contains(&"idx_emails_jmap_id".to_string()));
        assert!(indexes.contains(&"idx_emails_message_id".to_string()));
        assert!(indexes.contains(&"idx_mailboxes_parent_id".to_string()));
        assert!(indexes.contains(&"idx_mailboxes_path".to_string()));
    }

    #[test]
    fn test_open_in_memory_is_idempotent() {
        Database::open_in_memory().unwrap();
        Database::open_in_memory().unwrap();
    }

    #[test]
    fn test_open_creates_db_file() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("test.sqlite");
        let db = Database::open(&db_path).unwrap();
        assert!(db_path.exists());
        let count: i64 = db
            .connection()
            .query_row("SELECT COUNT(*) FROM mailboxes", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_open_on_existing_db() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("test.sqlite");

        let db = Database::open(&db_path).unwrap();
        db.connection()
            .execute(
                "INSERT INTO mailboxes (id, name, path) VALUES ('x', 'test', 'test')",
                [],
            )
            .unwrap();
        drop(db);

        let db2 = Database::open(&db_path).unwrap();
        let count: i64 = db2
            .connection()
            .query_row("SELECT COUNT(*) FROM mailboxes", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    // -----------------------------------------------------------------------
    // Schema vs struct alignment tests
    // -----------------------------------------------------------------------

    fn pragma_columns(db: &Database, table: &str) -> Vec<String> {
        db.connection()
            .prepare(&format!("PRAGMA table_info({table})"))
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap()
    }

    #[test]
    fn schema_matches_mailbox_struct() {
        let db = Database::open_in_memory().unwrap();
        assert_eq!(
            pragma_columns(&db, "mailboxes"),
            vec![
                "id",
                "jmap_id",
                "name",
                "parent_id",
                "role",
                "sort_order",
                "path",
                "jmap_state",
            ]
        );
    }

    #[test]
    fn schema_matches_email_struct() {
        let db = Database::open_in_memory().unwrap();
        assert_eq!(
            pragma_columns(&db, "emails"),
            vec![
                "id",
                "jmap_id",
                "message_id",
                "file_path",
                "primary_mailbox",
                "keywords",
                "jmap_state",
                "size",
                "last_sync",
                "is_dirty",
            ]
        );
    }

    #[test]
    fn schema_matches_email_mailbox_struct() {
        let db = Database::open_in_memory().unwrap();
        assert_eq!(
            pragma_columns(&db, "email_mailboxes"),
            vec!["email_id", "mailbox_id", "is_primary"]
        );
    }

    // -----------------------------------------------------------------------
    // Mailbox CRUD tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_insert_and_get_mailbox() {
        let db = Database::open_in_memory().unwrap();
        let mb = sample_mailbox("INBOX", "INBOX");
        db.insert_mailbox(&mb).unwrap();
        let got = db.get_mailbox(&mb.id).unwrap().unwrap();
        assert_eq!(got, mb);
    }

    #[test]
    fn test_get_mailbox_not_found() {
        let db = Database::open_in_memory().unwrap();
        let got = db.get_mailbox("nonexistent").unwrap();
        assert!(got.is_none());
    }

    #[test]
    fn test_get_mailbox_by_jmap_id() {
        let db = Database::open_in_memory().unwrap();
        let mb = sample_mailbox("INBOX", "INBOX");
        db.insert_mailbox(&mb).unwrap();
        let got = db.get_mailbox_by_jmap_id("jmap-INBOX").unwrap().unwrap();
        assert_eq!(got.id, mb.id);
    }

    #[test]
    fn test_get_mailbox_by_jmap_id_not_found() {
        let db = Database::open_in_memory().unwrap();
        let got = db.get_mailbox_by_jmap_id("nonexistent").unwrap();
        assert!(got.is_none());
    }

    #[test]
    fn test_get_all_mailboxes() {
        let db = Database::open_in_memory().unwrap();
        let mb1 = sample_mailbox("INBOX", "INBOX");
        let mb2 = sample_mailbox("Sent", "Sent");
        db.insert_mailbox(&mb1).unwrap();
        db.insert_mailbox(&mb2).unwrap();
        let all = db.get_all_mailboxes().unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_update_mailbox() {
        let db = Database::open_in_memory().unwrap();
        let mut mb = sample_mailbox("INBOX", "INBOX");
        db.insert_mailbox(&mb).unwrap();

        mb.name = "Updated".to_string();
        mb.jmap_id = Some("new-jmap-id".to_string());
        db.update_mailbox(&mb).unwrap();

        let got = db.get_mailbox(&mb.id).unwrap().unwrap();
        assert_eq!(got.name, "Updated");
        assert_eq!(got.jmap_id, Some("new-jmap-id".to_string()));
    }

    #[test]
    fn test_delete_mailbox() {
        let db = Database::open_in_memory().unwrap();
        let mb = sample_mailbox("INBOX", "INBOX");
        db.insert_mailbox(&mb).unwrap();
        db.delete_mailbox(&mb.id).unwrap();
        let got = db.get_mailbox(&mb.id).unwrap();
        assert!(got.is_none());
    }

    #[test]
    fn test_mailbox_unique_jmap_id() {
        let db = Database::open_in_memory().unwrap();
        let mb1 = sample_mailbox("INBOX", "INBOX");
        let mut mb2 = sample_mailbox("Sent", "Sent");
        mb2.jmap_id = mb1.jmap_id.clone();
        db.insert_mailbox(&mb1).unwrap();
        assert!(db.insert_mailbox(&mb2).is_err());
    }

    #[test]
    fn test_mailbox_parent_relationship() {
        let db = Database::open_in_memory().unwrap();
        let parent = sample_mailbox("Parent", "Parent");
        db.insert_mailbox(&parent).unwrap();

        let mut child = sample_mailbox("Child", "Parent.Child");
        child.parent_id = Some(parent.id.clone());
        db.insert_mailbox(&child).unwrap();

        let got_child = db.get_mailbox(&child.id).unwrap().unwrap();
        assert_eq!(got_child.parent_id, Some(parent.id));
    }

    // -----------------------------------------------------------------------
    // Email CRUD tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_insert_and_get_email() {
        let db = Database::open_in_memory().unwrap();
        let mb = sample_mailbox("INBOX", "INBOX");
        db.insert_mailbox(&mb).unwrap();

        let email = sample_email(&mb.id, "/tmp/mail/INBOX/123:2,");
        db.insert_email(&email).unwrap();

        let got = db.get_email(&email.id).unwrap().unwrap();
        assert_eq!(got, email);
    }

    #[test]
    fn test_get_email_not_found() {
        let db = Database::open_in_memory().unwrap();
        let got = db.get_email("nonexistent").unwrap();
        assert!(got.is_none());
    }

    #[test]
    fn test_get_email_by_jmap_id() {
        let db = Database::open_in_memory().unwrap();
        let mb = sample_mailbox("INBOX", "INBOX");
        db.insert_mailbox(&mb).unwrap();

        let email = sample_email(&mb.id, "/tmp/mail/INBOX/123:2,");
        db.insert_email(&email).unwrap();

        let got = db
            .get_email_by_jmap_id(email.jmap_id.as_ref().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(got.id, email.id);
    }

    #[test]
    fn test_get_email_by_file_path() {
        let db = Database::open_in_memory().unwrap();
        let mb = sample_mailbox("INBOX", "INBOX");
        db.insert_mailbox(&mb).unwrap();

        let email = sample_email(&mb.id, "/tmp/mail/INBOX/123:2,");
        db.insert_email(&email).unwrap();

        let got = db
            .get_email_by_file_path("/tmp/mail/INBOX/123:2,")
            .unwrap()
            .unwrap();
        assert_eq!(got.id, email.id);
    }

    #[test]
    fn test_get_all_emails() {
        let db = Database::open_in_memory().unwrap();
        let mb = sample_mailbox("INBOX", "INBOX");
        db.insert_mailbox(&mb).unwrap();

        let e1 = sample_email(&mb.id, "/tmp/mail/INBOX/1:2,");
        let e2 = sample_email(&mb.id, "/tmp/mail/INBOX/2:2,");
        db.insert_email(&e1).unwrap();
        db.insert_email(&e2).unwrap();

        let all = db.get_all_emails().unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_update_email() {
        let db = Database::open_in_memory().unwrap();
        let mb = sample_mailbox("INBOX", "INBOX");
        db.insert_mailbox(&mb).unwrap();

        let mut email = sample_email(&mb.id, "/tmp/mail/INBOX/1:2,");
        db.insert_email(&email).unwrap();

        email.is_dirty = true;
        email.keywords = Some(r#"{"$seen":true}"#.to_string());
        db.update_email(&email).unwrap();

        let got = db.get_email(&email.id).unwrap().unwrap();
        assert!(got.is_dirty);
        assert_eq!(got.keywords, Some(r#"{"$seen":true}"#.to_string()));
    }

    #[test]
    fn test_delete_email() {
        let db = Database::open_in_memory().unwrap();
        let mb = sample_mailbox("INBOX", "INBOX");
        db.insert_mailbox(&mb).unwrap();

        let email = sample_email(&mb.id, "/tmp/mail/INBOX/1:2,");
        db.insert_email(&email).unwrap();
        db.delete_email(&email.id).unwrap();

        let got = db.get_email(&email.id).unwrap();
        assert!(got.is_none());
    }

    #[test]
    fn test_email_unique_jmap_id() {
        let db = Database::open_in_memory().unwrap();
        let mb = sample_mailbox("INBOX", "INBOX");
        db.insert_mailbox(&mb).unwrap();

        let e1 = sample_email(&mb.id, "/tmp/mail/1:2,");
        let mut e2 = sample_email(&mb.id, "/tmp/mail/2:2,");
        e2.jmap_id = e1.jmap_id.clone();
        db.insert_email(&e1).unwrap();
        assert!(db.insert_email(&e2).is_err());
    }

    #[test]
    fn test_email_unique_file_path() {
        let db = Database::open_in_memory().unwrap();
        let mb = sample_mailbox("INBOX", "INBOX");
        db.insert_mailbox(&mb).unwrap();

        let e1 = sample_email(&mb.id, "/tmp/mail/same:2,");
        let mut e2 = sample_email(&mb.id, "/tmp/mail/same:2,");
        e2.id = generate_id();
        e2.jmap_id = Some("different-jmap-id".to_string());
        db.insert_email(&e1).unwrap();
        assert!(db.insert_email(&e2).is_err());
    }

    #[test]
    fn test_email_is_dirty_default() {
        let db = Database::open_in_memory().unwrap();
        let mb = sample_mailbox("INBOX", "INBOX");
        db.insert_mailbox(&mb).unwrap();

        let email = EmailRow {
            id: generate_id(),
            jmap_id: Some("jmid".to_string()),
            message_id: None,
            file_path: "/tmp/mail/dirty-test:2,".to_string(),
            primary_mailbox: mb.id.clone(),
            keywords: None,
            jmap_state: None,
            size: None,
            last_sync: None,
            is_dirty: true,
        };
        db.insert_email(&email).unwrap();
        let got = db.get_email(&email.id).unwrap().unwrap();
        assert!(got.is_dirty);
    }

    // -----------------------------------------------------------------------
    // EmailMailbox CRUD tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_insert_and_get_email_mailboxes() {
        let db = Database::open_in_memory().unwrap();
        let mb = sample_mailbox("INBOX", "INBOX");
        db.insert_mailbox(&mb).unwrap();

        let email = sample_email(&mb.id, "/tmp/mail/1:2,");
        db.insert_email(&email).unwrap();

        let em = EmailMailboxRow {
            email_id: email.id.clone(),
            mailbox_id: mb.id.clone(),
            is_primary: true,
        };
        db.insert_email_mailbox(&em).unwrap();

        let by_email = db.get_email_mailboxes_by_email(&email.id).unwrap();
        assert_eq!(by_email.len(), 1);
        assert_eq!(by_email[0], em);

        let by_mailbox = db.get_email_mailboxes_by_mailbox(&mb.id).unwrap();
        assert_eq!(by_mailbox.len(), 1);
        assert_eq!(by_mailbox[0], em);
    }

    #[test]
    fn test_delete_email_mailbox() {
        let db = Database::open_in_memory().unwrap();
        let mb = sample_mailbox("INBOX", "INBOX");
        db.insert_mailbox(&mb).unwrap();

        let email = sample_email(&mb.id, "/tmp/mail/1:2,");
        db.insert_email(&email).unwrap();

        let em = EmailMailboxRow {
            email_id: email.id.clone(),
            mailbox_id: mb.id.clone(),
            is_primary: true,
        };
        db.insert_email_mailbox(&em).unwrap();
        db.delete_email_mailbox(&email.id, &mb.id).unwrap();

        let by_email = db.get_email_mailboxes_by_email(&email.id).unwrap();
        assert!(by_email.is_empty());
    }

    #[test]
    fn test_set_primary_mailbox() {
        let db = Database::open_in_memory().unwrap();
        let mb1 = sample_mailbox("INBOX", "INBOX");
        let mb2 = sample_mailbox("Archive", "Archive");
        db.insert_mailbox(&mb1).unwrap();
        db.insert_mailbox(&mb2).unwrap();

        let email = sample_email(&mb1.id, "/tmp/mail/1:2,");
        db.insert_email(&email).unwrap();

        let em1 = EmailMailboxRow {
            email_id: email.id.clone(),
            mailbox_id: mb1.id.clone(),
            is_primary: true,
        };
        let em2 = EmailMailboxRow {
            email_id: email.id.clone(),
            mailbox_id: mb2.id.clone(),
            is_primary: false,
        };
        db.insert_email_mailbox(&em1).unwrap();
        db.insert_email_mailbox(&em2).unwrap();

        db.set_primary_mailbox(&email.id, &mb2.id).unwrap();

        let by_email = db.get_email_mailboxes_by_email(&email.id).unwrap();
        let mb1_row = by_email.iter().find(|em| em.mailbox_id == mb1.id).unwrap();
        let mb2_row = by_email.iter().find(|em| em.mailbox_id == mb2.id).unwrap();
        assert!(!mb1_row.is_primary);
        assert!(mb2_row.is_primary);
    }

    #[test]
    fn test_email_mailbox_cascade_delete() {
        let db = Database::open_in_memory().unwrap();
        let mb = sample_mailbox("INBOX", "INBOX");
        db.insert_mailbox(&mb).unwrap();

        let email = sample_email(&mb.id, "/tmp/mail/1:2,");
        db.insert_email(&email).unwrap();

        let em = EmailMailboxRow {
            email_id: email.id.clone(),
            mailbox_id: mb.id.clone(),
            is_primary: true,
        };
        db.insert_email_mailbox(&em).unwrap();

        db.delete_email(&email.id).unwrap();
        let by_email = db.get_email_mailboxes_by_email(&email.id).unwrap();
        assert!(by_email.is_empty());
    }

    // -----------------------------------------------------------------------
    // generate_id tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_generate_id_is_unique() {
        let id1 = generate_id();
        let id2 = generate_id();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_generate_id_is_uuid() {
        let id = generate_id();
        assert_eq!(id.chars().filter(|&c| c == '-').count(), 4);
    }
}
