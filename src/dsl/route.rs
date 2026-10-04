//! 网络与目录共用的路径模式。

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Seg {
    Lit(String),
    /// suffix 为空表示整段都是参数。
    Param { name: String, suffix: String },
    Star,
    Rest,
    RestParam(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pattern {
    pub segs: Vec<Seg>,
    pub captures: Vec<String>,
}

pub fn parse_pattern(text: &str) -> Result<Pattern, String> {
    if !text.starts_with('/') {
        return Err("路径要以 / 开头".into());
    }
    if text.contains("//") {
        return Err("路径里不能有空段".into());
    }
    let body = text.trim_end_matches('/');
    let raw: Vec<&str> = if body == "" || body == "/" {
        Vec::new()
    } else {
        body[1..].split('/').collect()
    };
    let mut segs = Vec::new();
    let mut captures = Vec::new();
    let count = raw.len();
    for (index, part) in raw.into_iter().enumerate() {
        let last = index + 1 == count;
        let seg = if part == "**" {
            if !last {
                return Err("** 只能写在路径最后".into());
            }
            Seg::Rest
        } else if let Some(name) = part.strip_prefix("{**").and_then(|rest| rest.strip_suffix('}')) {
            if !last {
                return Err("** 只能写在路径最后".into());
            }
            check_name(name)?;
            if captures.iter().any(|item| item == name) {
                return Err(format!("参数 {name} 重复"));
            }
            captures.push(name.to_string());
            Seg::RestParam(name.to_string())
        } else if part == "*" {
            Seg::Star
        } else if part.starts_with('{') {
            let Some(end) = part.find('}') else {
                return Err(format!("参数没有写完: {part}"));
            };
            let name = &part[1..end];
            check_name(name)?;
            if captures.iter().any(|item| item == name) {
                return Err(format!("参数 {name} 重复"));
            }
            let suffix = &part[end + 1..];
            if suffix.contains('{') || suffix.contains('}') {
                return Err(format!("这一段写错了: {part}"));
            }
            captures.push(name.to_string());
            Seg::Param { name: name.to_string(), suffix: suffix.to_string() }
        } else if part.contains('{') || part.contains('}') || part == "." || part == ".." {
            return Err(format!("这一段写错了: {part}"));
        } else {
            Seg::Lit(part.to_string())
        };
        segs.push(seg);
    }
    Ok(Pattern { segs, captures })
}

fn check_name(name: &str) -> Result<(), String> {
    let mut chars = name.chars();
    match chars.next() {
        Some(ch) if ch.is_ascii_alphabetic() || ch == '_' => {}
        _ => return Err(format!("参数名不合法: {name}")),
    }
    if chars.any(|ch| !(ch.is_ascii_alphanumeric() || ch == '_')) {
        return Err(format!("参数名不合法: {name}"));
    }
    Ok(())
}

fn rank(seg: &Seg) -> u8 {
    match seg {
        Seg::Lit(_) => 4,
        Seg::Param { suffix, .. } if !suffix.is_empty() => 3,
        Seg::Param { .. } => 2,
        Seg::Star => 1,
        Seg::Rest | Seg::RestParam(_) => 0,
    }
}

/// 两条模式若存在一条路径让特异度比完仍相同，则返回 true。
pub fn ambiguous(left: &Pattern, right: &Pattern) -> bool {
    tie(&left.segs, &right.segs)
}

fn tie(left: &[Seg], right: &[Seg]) -> bool {
    match (left.first(), right.first()) {
        (None, None) => true,
        (Some(Seg::Rest | Seg::RestParam(_)), Some(Seg::Rest | Seg::RestParam(_))) => true,
        (Some(Seg::Rest | Seg::RestParam(_)), _) | (_, Some(Seg::Rest | Seg::RestParam(_))) => false,
        (None, Some(_)) | (Some(_), None) => false,
        (Some(a), Some(b)) => {
            if !overlap_seg(a, b) {
                return false;
            }
            if rank(a) != rank(b) {
                return false;
            }
            tie(&left[1..], &right[1..])
        }
    }
}

fn overlap_seg(left: &Seg, right: &Seg) -> bool {
    match (left, right) {
        (Seg::Lit(a), Seg::Lit(b)) => a == b,
        (Seg::Lit(text), Seg::Param { suffix, .. }) | (Seg::Param { suffix, .. }, Seg::Lit(text)) => {
            text.ends_with(suffix) && text.len() > suffix.len() && !reserved(text.trim_end_matches(suffix))
        }
        (Seg::Param { suffix: a, .. }, Seg::Param { suffix: b, .. }) => a == b || a.is_empty() || b.is_empty(),
        (Seg::Lit(_), Seg::Star) | (Seg::Star, Seg::Lit(_)) => true,
        (Seg::Param { .. }, Seg::Star) | (Seg::Star, Seg::Param { .. }) => true,
        (Seg::Star, Seg::Star) => true,
        _ => false,
    }
}

fn reserved(text: &str) -> bool {
    text.is_empty() || text == "." || text == ".."
}

pub fn match_pattern(pattern: &Pattern, path: &str) -> Option<Vec<String>> {
    let parts: Vec<&str> = if path.is_empty() {
        Vec::new()
    } else {
        path.split('/').collect()
    };
    let mut caps = Vec::new();
    if !eat(&pattern.segs, &parts, &mut caps) {
        return None;
    }
    Some(caps)
}

fn eat(segs: &[Seg], parts: &[&str], caps: &mut Vec<String>) -> bool {
    let Some(seg) = segs.first() else {
        return parts.is_empty();
    };
    match seg {
        Seg::Rest => !parts.is_empty() && segs.len() == 1,
        Seg::RestParam(_) => {
            if parts.is_empty() || segs.len() != 1 {
                return false;
            }
            caps.push(parts.join("/"));
            true
        }
        _ => {
            let Some(part) = parts.first() else {
                return false;
            };
            if !take(seg, part, caps) {
                return false;
            }
            eat(&segs[1..], &parts[1..], caps)
        }
    }
}

fn take(seg: &Seg, part: &str, caps: &mut Vec<String>) -> bool {
    if part.is_empty() {
        return false;
    }
    match seg {
        Seg::Lit(text) => part == text,
        Seg::Param { suffix, .. } => {
            if !part.ends_with(suffix) || part.len() <= suffix.len() {
                return false;
            }
            let name = &part[..part.len() - suffix.len()];
            if reserved(name) {
                return false;
            }
            caps.push(name.to_string());
            true
        }
        Seg::Star => !reserved(part),
        Seg::Rest | Seg::RestParam(_) => false,
    }
}

/// 运行时在已匹配的模式里选特异度更高的一条。相等则返回 None。
pub fn more_specific(left: &Pattern, right: &Pattern) -> Option<std::cmp::Ordering> {
    use std::cmp::Ordering;
    let n = left.segs.len().max(right.segs.len());
    for index in 0..n {
        match (left.segs.get(index), right.segs.get(index)) {
            (Some(a), Some(b)) if rank(a) != rank(b) => {
                return Some(rank(a).cmp(&rank(b)));
            }
            (Some(_), None) => return Some(Ordering::Greater),
            (None, Some(_)) => return Some(Ordering::Less),
            _ => {}
        }
    }
    None
}
