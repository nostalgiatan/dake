use super::executor::ExecutionError;
use crate::dsl::ir::RouteIr;
use crate::dsl::route::{match_pattern, more_specific};
use std::cmp::Ordering;

/// 在已经匹配的模式里只保留最具体的一条，避免为每条候选再排序。
pub(super) fn pick<'a>(routes: &'a [RouteIr], source: &str, url: bool, rel: &str) -> Option<(&'a RouteIr, Vec<String>)> {
    let mut found: Option<(&RouteIr, Vec<String>)> = None;
    let mut fallback = None;
    for route in routes.iter().filter(|route| route.source == source && route.url == url) {
        let Some(pattern) = &route.pattern else {
            fallback = Some(route);
            continue;
        };
        let Some(caps) = match_pattern(pattern, rel) else { continue };
        match &found {
            None => found = Some((route, caps)),
            Some((winner, _)) => {
                let Some(previous) = &winner.pattern else { continue };
                match more_specific(pattern, previous) {
                    Some(Ordering::Greater) => found = Some((route, caps)),
                    Some(Ordering::Less) => {}
                    Some(Ordering::Equal) | None => {
                        eprintln!("{}", ExecutionError::new(4036, format!("{source} 的路由特异度相同")));
                        return None;
                    }
                }
            }
        }
    }
    found.or_else(|| fallback.map(|route| (route, Vec::new())))
}
