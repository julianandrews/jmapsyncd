use anyhow::Result;

const FLAG_ORDER: &[char] = &['D', 'F', 'P', 'R', 'S', 'T'];

fn flag_index(c: char) -> Option<usize> {
    match c {
        'D' => Some(0),
        'F' => Some(1),
        'P' => Some(2),
        'R' => Some(3),
        'S' => Some(4),
        'T' => Some(5),
        _ => None,
    }
}

/// Parse the flags portion of a Maildir info suffix (`:2,{flags}`).
/// Accepts the full suffix or just the flags part.
/// Returns a normalized (sorted, deduplicated, validated) flags string.
pub fn parse_flags(input: &str) -> Result<String> {
    let raw = input.strip_prefix(":2,").unwrap_or(input);
    normalize_flags(raw)
}

/// Format a flags string as a `:2,{flags}` info suffix.
/// The flags are normalized even if already valid.
pub fn flags_to_suffix(flags: &str) -> String {
    let normalized = normalize_flags_lossy(flags);
    format!(":2,{normalized}")
}

/// Normalize a flags string: validate, sort, deduplicate.
pub fn normalize_flags(raw: &str) -> Result<String> {
    let mut seen = [false; 6];
    for (i, c) in raw.chars().enumerate() {
        let idx = flag_index(c)
            .ok_or_else(|| anyhow::anyhow!("invalid Maildir flag {c:?} at position {i}"))?;
        seen[idx] = true;
    }
    let mut out = String::with_capacity(6);
    for (i, &c) in FLAG_ORDER.iter().enumerate() {
        if seen[i] {
            out.push(c);
        }
    }
    Ok(out)
}

/// Normalize flags silently (invalid chars are discarded).
pub fn normalize_flags_lossy(raw: &str) -> String {
    let mut seen = [false; 6];
    for c in raw.chars() {
        if let Some(idx) = flag_index(c) {
            seen[idx] = true;
        }
    }
    let mut out = String::with_capacity(6);
    for (i, &c) in FLAG_ORDER.iter().enumerate() {
        if seen[i] {
            out.push(c);
        }
    }
    out
}

/// Parsed components of a Maildir filename.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilenameParts {
    pub timestamp: String,
    pub uniquifier: String,
    pub flags: String,
}

/// Parse a Maildir filename into its components.
/// Accepts filenames with or without the `:2,{flags}` info suffix.
pub fn parse_filename(filename: &str) -> Option<FilenameParts> {
    let (base, suffix) = if let Some(pos) = filename.find(":2,") {
        let (b, s) = filename.split_at(pos);
        (b, s)
    } else {
        (filename, "")
    };

    let (timestamp, uniquifier) = base.split_once('.')?;
    if timestamp.is_empty() || uniquifier.is_empty() {
        return None;
    }

    let flags = if suffix.is_empty() {
        String::new()
    } else {
        normalize_flags_lossy(suffix.strip_prefix(":2,").unwrap_or(""))
    };

    Some(FilenameParts {
        timestamp: timestamp.to_string(),
        uniquifier: uniquifier.to_string(),
        flags,
    })
}

/// Generate a Maildir filename from its parts.
pub fn generate_filename(parts: &FilenameParts) -> String {
    let flags = normalize_flags_lossy(&parts.flags);
    format!("{}.{}:2,{flags}", parts.timestamp, parts.uniquifier)
}

/// JMAP keyword to Maildir flag mapping.
const KEYWORD_FLAG_MAP: &[(&str, char)] = &[
    ("$answered", 'R'),
    ("$draft", 'D'),
    ("$flagged", 'F'),
    ("$forwarded", 'P'),
    ("$seen", 'S'),
    ("$trashed", 'T'),
];

const FLAG_KEYWORD_MAP: &[(char, &str)] = &[
    ('D', "$draft"),
    ('F', "$flagged"),
    ('P', "$forwarded"),
    ('R', "$answered"),
    ('S', "$seen"),
    ('T', "$trashed"),
];

/// Convert a JMAP keyword to a Maildir flag character, if applicable.
pub fn keyword_to_flag(keyword: &str) -> Option<char> {
    KEYWORD_FLAG_MAP
        .iter()
        .find(|(k, _)| *k == keyword)
        .map(|&(_, f)| f)
}

/// Convert a Maildir flag character to a JMAP keyword, if applicable.
pub fn flag_to_keyword(flag: char) -> Option<&'static str> {
    FLAG_KEYWORD_MAP
        .iter()
        .find(|(f, _)| *f == flag)
        .map(|&(_, k)| k)
}

/// Convert a set of JMAP keywords to a sorted, deduplicated Maildir flags string.
pub fn keywords_to_flags(keywords: &std::collections::BTreeSet<String>) -> String {
    let mut seen = [false; 6];
    for kw in keywords {
        if let Some(&(_, f)) = KEYWORD_FLAG_MAP.iter().find(|(k, _)| *k == kw.as_str()) {
            if let Some(idx) = flag_index(f) {
                seen[idx] = true;
            }
        }
    }
    let mut out = String::with_capacity(6);
    for (i, &c) in FLAG_ORDER.iter().enumerate() {
        if seen[i] {
            out.push(c);
        }
    }
    out
}

/// Convert a Maildir flags string to a vector of JMAP keywords.
pub fn flags_to_keywords(flags: &str) -> Vec<String> {
    let mut result = Vec::new();
    for c in flags.chars() {
        if let Some(&(_, k)) = FLAG_KEYWORD_MAP.iter().find(|(f, _)| *f == c) {
            result.push(k.to_string());
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_empty_flags() {
        assert_eq!(parse_flags("").unwrap(), "");
        assert_eq!(parse_flags(":2,").unwrap(), "");
    }

    #[test]
    fn parse_single_flag() {
        assert_eq!(parse_flags("S").unwrap(), "S");
        assert_eq!(parse_flags(":2,S").unwrap(), "S");
    }

    #[test]
    fn parse_multiple_flags_sorted() {
        assert_eq!(parse_flags("RS").unwrap(), "RS");
    }

    #[test]
    fn parse_multiple_flags_unsorted() {
        assert_eq!(parse_flags("SR").unwrap(), "RS");
    }

    #[test]
    fn parse_duplicate_flags() {
        assert_eq!(parse_flags("SS").unwrap(), "S");
    }

    #[test]
    fn parse_all_flags() {
        assert_eq!(parse_flags("DFPRST").unwrap(), "DFPRST");
    }

    #[test]
    fn parse_invalid_flag() {
        assert!(parse_flags("X").is_err());
        assert!(parse_flags("SX").is_err());
    }

    #[test]
    fn parse_invalid_flag_lowercase() {
        assert!(parse_flags("s").is_err());
    }

    #[test]
    fn flags_to_suffix_normalized() {
        assert_eq!(flags_to_suffix("SR"), ":2,RS");
        assert_eq!(flags_to_suffix(""), ":2,");
        assert_eq!(flags_to_suffix("S"), ":2,S");
    }

    #[test]
    fn flags_to_suffix_ignores_invalid() {
        assert_eq!(flags_to_suffix("SX"), ":2,S");
    }

    #[test]
    fn parse_filename_with_suffix() {
        let parts = parse_filename("1700000000.host123:2,S").unwrap();
        assert_eq!(parts.timestamp, "1700000000");
        assert_eq!(parts.uniquifier, "host123");
        assert_eq!(parts.flags, "S");
    }

    #[test]
    fn parse_filename_without_suffix() {
        let parts = parse_filename("1700000000.host123").unwrap();
        assert_eq!(parts.timestamp, "1700000000");
        assert_eq!(parts.uniquifier, "host123");
        assert_eq!(parts.flags, "");
    }

    #[test]
    fn parse_filename_with_multiple_dots() {
        let parts = parse_filename("1700000000.host.pid123:2,RS").unwrap();
        assert_eq!(parts.timestamp, "1700000000");
        assert_eq!(parts.uniquifier, "host.pid123");
        assert_eq!(parts.flags, "RS");
    }

    #[test]
    fn parse_filename_empty_timestamp() {
        assert!(parse_filename(".host:2,S").is_none());
    }

    #[test]
    fn parse_filename_empty_uniquifier() {
        assert!(parse_filename("1700000000.:2,S").is_none());
    }

    #[test]
    fn parse_filename_no_dot() {
        assert!(parse_filename("no_dot").is_none());
    }

    #[test]
    fn parse_filename_empty() {
        assert!(parse_filename("").is_none());
    }

    #[test]
    fn generate_filename_roundtrip() {
        let parts = FilenameParts {
            timestamp: "1700000000".to_string(),
            uniquifier: "host123".to_string(),
            flags: "S".to_string(),
        };
        let filename = generate_filename(&parts);
        assert_eq!(filename, "1700000000.host123:2,S");
        let parsed = parse_filename(&filename).unwrap();
        assert_eq!(parsed, parts);
    }

    #[test]
    fn generate_filename_no_flags() {
        let parts = FilenameParts {
            timestamp: "1700000000".to_string(),
            uniquifier: "host123".to_string(),
            flags: String::new(),
        };
        let filename = generate_filename(&parts);
        assert_eq!(filename, "1700000000.host123:2,");
    }

    #[test]
    fn generate_filename_unsorted_flags() {
        let parts = FilenameParts {
            timestamp: "1700000000".to_string(),
            uniquifier: "host123".to_string(),
            flags: "SR".to_string(),  // unsorted, should be normalized
        };
        let filename = generate_filename(&parts);
        assert_eq!(filename, "1700000000.host123:2,RS");
    }

    #[test]
    fn keyword_to_flag_mapping() {
        assert_eq!(keyword_to_flag("$seen"), Some('S'));
        assert_eq!(keyword_to_flag("$flagged"), Some('F'));
        assert_eq!(keyword_to_flag("$draft"), Some('D'));
        assert_eq!(keyword_to_flag("$answered"), Some('R'));
        assert_eq!(keyword_to_flag("$trashed"), Some('T'));
        assert_eq!(keyword_to_flag("$forwarded"), Some('P'));
        assert_eq!(keyword_to_flag("$unknown"), None);
    }

    #[test]
    fn flag_to_keyword_mapping() {
        assert_eq!(flag_to_keyword('S'), Some("$seen"));
        assert_eq!(flag_to_keyword('F'), Some("$flagged"));
        assert_eq!(flag_to_keyword('D'), Some("$draft"));
        assert_eq!(flag_to_keyword('R'), Some("$answered"));
        assert_eq!(flag_to_keyword('T'), Some("$trashed"));
        assert_eq!(flag_to_keyword('P'), Some("$forwarded"));
        assert_eq!(flag_to_keyword('X'), None);
    }

    #[test]
    fn keywords_to_flags_empty() {
        let set = std::collections::BTreeSet::new();
        assert_eq!(keywords_to_flags(&set), "");
    }

    #[test]
    fn keywords_to_flags_single() {
        let mut set = std::collections::BTreeSet::new();
        set.insert("$seen".to_string());
        assert_eq!(keywords_to_flags(&set), "S");
    }

    #[test]
    fn keywords_to_flags_multiple() {
        let mut set = std::collections::BTreeSet::new();
        set.insert("$seen".to_string());
        set.insert("$flagged".to_string());
        set.insert("$draft".to_string());
        assert_eq!(keywords_to_flags(&set), "DFS");
    }

    #[test]
    fn keywords_to_flags_unknown_ignored() {
        let mut set = std::collections::BTreeSet::new();
        set.insert("$seen".to_string());
        set.insert("$custom".to_string());
        assert_eq!(keywords_to_flags(&set), "S");
    }

    #[test]
    fn flags_to_keywords_empty() {
        assert!(flags_to_keywords("").is_empty());
    }

    #[test]
    fn flags_to_keywords_single() {
        assert_eq!(flags_to_keywords("S"), vec!["$seen"]);
    }

    #[test]
    fn flags_to_keywords_multiple() {
        let result = flags_to_keywords("DF");
        assert_eq!(result, vec!["$draft", "$flagged"]);
    }

    #[test]
    fn flags_to_keywords_unknown_ignored() {
        let result = flags_to_keywords("SX");
        assert_eq!(result, vec!["$seen"]);
    }

    #[test]
    fn keyword_flag_roundtrip() {
        let keywords = ["$seen", "$flagged", "$draft", "$answered", "$trashed", "$forwarded"];
        for kw in &keywords {
            let flag = keyword_to_flag(kw).unwrap();
            let kw_back = flag_to_keyword(flag).unwrap();
            assert_eq!(kw_back, *kw);
        }
    }

    #[test]
    fn normalize_flags_sorts() {
        assert_eq!(normalize_flags("TSRFPD").unwrap(), "DFPRST");
    }

    #[test]
    fn normalize_flags_rejects_invalid() {
        assert!(normalize_flags("SX").is_err());
    }

    #[test]
    fn normalize_flags_rejects_lowercase() {
        assert!(normalize_flags("s").is_err());
    }

    #[test]
    fn normalize_flags_lossy_ignores_invalid() {
        assert_eq!(normalize_flags_lossy("SX"), "S");
        assert_eq!(normalize_flags_lossy("s"), "");
    }
}
