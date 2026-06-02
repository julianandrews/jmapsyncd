use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MailboxRow {
    pub id: String,
    pub jmap_id: Option<String>,
    pub name: String,
    pub parent_id: Option<String>,
    pub role: Option<String>,
    pub sort_order: Option<i64>,
    pub path: String,
    pub jmap_state: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmailRow {
    pub id: String,
    pub jmap_id: Option<String>,
    pub message_id: Option<String>,
    pub file_path: String,
    pub primary_mailbox: String,
    pub keywords: Option<String>,
    pub jmap_state: Option<String>,
    pub size: Option<i64>,
    pub last_sync: Option<i64>,
    pub is_dirty: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmailMailboxRow {
    pub email_id: String,
    pub mailbox_id: String,
    pub is_primary: bool,
}
