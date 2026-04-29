//! Filesystem helpers: filename sanitization + default download directory.

use std::path::PathBuf;

/// Sanitize a filename received from the peer.
///
/// Rules:
/// - Strip path separators (`/` and `\`) — collapses to last component
/// - Replace control chars (< 0x20) and Windows-reserved chars `< > : " | ? *`
///   with `_`
/// - Collapse a fully-empty result to "untitled"
/// - Reject Windows reserved device names (CON, PRN, AUX, NUL, COMx, LPTx)
///   by prefixing `_`
/// - Cap length to 200 characters (Windows MAX_PATH wiggle room)
pub fn sanitize_filename(input: &str) -> String {
    // Take only the last path segment.
    let last_segment = input
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("");

    let mut out = String::with_capacity(last_segment.len());
    for c in last_segment.chars() {
        let bad = (c as u32) < 0x20
            || c == '<'
            || c == '>'
            || c == ':'
            || c == '"'
            || c == '|'
            || c == '?'
            || c == '*';
        out.push(if bad { '_' } else { c });
    }

    let trimmed = out.trim_matches(['.', ' ']);
    let mut result = if trimmed.is_empty() {
        "untitled".to_string()
    } else {
        trimmed.to_string()
    };

    // Windows reserved device names — block if the *stem* matches.
    let stem = result
        .split('.')
        .next()
        .unwrap_or("")
        .to_ascii_uppercase();
    let reserved = matches!(
        stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL"
    ) || (stem.starts_with("COM") || stem.starts_with("LPT"))
        && stem
            .chars()
            .skip(3)
            .all(|c| c.is_ascii_digit())
        && stem.len() > 3;
    if reserved {
        result.insert(0, '_');
    }

    // Length cap (chars, not bytes — Unicode-safe).
    if result.chars().count() > 200 {
        result = result.chars().take(200).collect();
    }
    result
}

/// Default download directory: `~/Downloads/Wormhole/`. Used by the host
/// to seed Config; pure helper so the host doesn't need its own copy of
/// the home-dir lookup.
pub fn default_download_dir() -> PathBuf {
    if let Some(home) = dirs_home() {
        return home.join("Downloads").join("Wormhole");
    }
    PathBuf::from(".")
}

/// Best-effort: pick a path inside `dir` that doesn't already exist. If the
/// chosen name collides, append `-1`, `-2`, … to the stem.
pub fn pick_save_path(suggested: &str, dir: &std::path::Path) -> PathBuf {
    let _ = std::fs::create_dir_all(dir);
    let safe = sanitize_filename(suggested);
    let candidate = dir.join(&safe);
    if !candidate.exists() {
        return candidate;
    }
    let (stem, ext) = match safe.rfind('.') {
        Some(i) if i > 0 => (&safe[..i], &safe[i..]),
        _ => (safe.as_str(), ""),
    };
    for i in 1..=999 {
        let alt = dir.join(format!("{stem}-{i}{ext}"));
        if !alt.exists() {
            return alt;
        }
    }
    // Fallback: just overwrite (extremely unlikely).
    candidate
}

#[cfg(windows)]
fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").map(PathBuf::from)
}

#[cfg(not(windows))]
fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_path_traversal() {
        assert_eq!(sanitize_filename("../etc/passwd"), "passwd");
        assert_eq!(sanitize_filename("..\\..\\Windows\\evil.exe"), "evil.exe");
    }

    #[test]
    fn replaces_reserved_chars() {
        assert_eq!(sanitize_filename("a:b|c?d.txt"), "a_b_c_d.txt");
    }

    #[test]
    fn empty_falls_back() {
        assert_eq!(sanitize_filename(""), "untitled");
        assert_eq!(sanitize_filename("..."), "untitled");
    }

    #[test]
    fn unicode_preserved() {
        assert_eq!(sanitize_filename("测试-中文_🔒.bin"), "测试-中文_🔒.bin");
    }

    #[test]
    fn windows_reserved_prefixed() {
        assert_eq!(sanitize_filename("CON"), "_CON");
        assert_eq!(sanitize_filename("COM1.txt"), "_COM1.txt");
        // LPT without a digit is fine.
        assert_eq!(sanitize_filename("LPT.txt"), "LPT.txt");
    }
}
