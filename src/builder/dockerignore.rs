use std::path::Path;

pub struct DockerIgnore {
    patterns: Vec<String>,
}

impl DockerIgnore {
    pub fn load_from_context(context_dir: &Path) -> Self {
        let ignore_file = context_dir.join(".dockerignore");
        if let Ok(content) = std::fs::read_to_string(&ignore_file) {
            let patterns: Vec<String> = content
                .lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .collect();
            Self { patterns }
        } else {
            Self {
                patterns: Vec::new(),
            }
        }
    }

    pub fn is_ignored(&self, rel_path: &Path) -> bool {
        let path_str = rel_path.to_string_lossy();
        let mut ignored = false;
        for p in &self.patterns {
            if let Some(exception) = p.strip_prefix('!') {
                if pattern_matches(exception, &path_str) {
                    ignored = false;
                }
            } else if pattern_matches(p, &path_str) {
                ignored = true;
            }
        }
        ignored
    }
}

fn pattern_matches(pattern: &str, path: &str) -> bool {
    let p = pattern.trim_end_matches('/');
    if p.contains('*') {
        let parts: Vec<&str> = p.split('*').collect();
        if parts.len() == 2 {
            let prefix = parts[0];
            let suffix = parts[1];
            return path.starts_with(prefix)
                && path.ends_with(suffix)
                && path.len() >= prefix.len() + suffix.len();
        }
    }
    path == p || path.starts_with(&format!("{}/", p)) || path.ends_with(&format!("/{}", p))
}

pub fn matches_wildcard(pattern: &str, text: &str) -> bool {
    let p_chars: Vec<char> = pattern.chars().collect();
    let t_chars: Vec<char> = text.chars().collect();
    let mut p = 0;
    let mut t = 0;
    let mut star = None;
    let mut match_idx = 0;

    while t < t_chars.len() {
        if p < p_chars.len() && (p_chars[p] == '?' || p_chars[p] == t_chars[t]) {
            p += 1;
            t += 1;
        } else if p < p_chars.len() && p_chars[p] == '*' {
            star = Some(p);
            match_idx = t;
            p += 1;
        } else if let Some(s) = star {
            p = s + 1;
            match_idx += 1;
            t = match_idx;
        } else {
            return false;
        }
    }
    while p < p_chars.len() && p_chars[p] == '*' {
        p += 1;
    }
    p == p_chars.len()
}
