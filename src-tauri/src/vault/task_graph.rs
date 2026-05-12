//! Circular dependency detection for task `blocked_by` chains.
//!
//! Per [ADR 0115 §4](../../../docs/adr/0115-tasks-and-projects-as-typed-notes.md), v1
//! detects cycles in a best-effort, scoped way: starting from the task being saved,
//! follow `blocked_by` edges; if any path leads back to the original task within
//! `max_depth` hops, it is a cycle. We do NOT walk the full vault graph or detect
//! cycles that don't touch the task being saved — that's vault-scope work for a
//! future ADR.

use std::collections::HashSet;

/// Default depth cap. 32 is comfortably larger than any reasonable dependency chain
/// while keeping the walk bounded against pathological vaults.
pub const DEFAULT_MAX_DEPTH: u32 = 32;

/// True when the proposed `blocked_by` for `task_name` introduces a cycle.
///
/// `resolver` returns the `blocked_by` list of any task referenced by wikilink target
/// during the walk. A target with no blocked_by (or one that can't be resolved) is
/// terminal — the walk continues to other branches.
///
/// Depth-bounded: chains longer than `max_depth` return `false` (no cycle reached).
pub fn has_blocked_by_cycle(
    task_name: &str,
    blocked_by: &[String],
    resolver: impl Fn(&str) -> Vec<String>,
    max_depth: u32,
) -> bool {
    let mut visited: HashSet<String> = HashSet::new();
    let mut stack: Vec<(String, u32)> = blocked_by
        .iter()
        .map(|target| (target.clone(), 1u32))
        .collect();

    while let Some((target, depth)) = stack.pop() {
        if target == task_name {
            return true;
        }
        if depth >= max_depth {
            continue;
        }
        if !visited.insert(target.clone()) {
            continue;
        }
        for next in resolver(&target) {
            stack.push((next, depth + 1));
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn resolver<'a>(
        map: &'a HashMap<&'static str, Vec<&'static str>>,
    ) -> impl Fn(&str) -> Vec<String> + 'a {
        move |target: &str| {
            map.get(target)
                .map(|v| v.iter().map(|s| s.to_string()).collect())
                .unwrap_or_default()
        }
    }

    #[test]
    fn linear_chain_has_no_cycle() {
        let mut map = HashMap::new();
        map.insert("B", vec!["C"]);
        map.insert("C", vec!["D"]);
        map.insert("D", vec![]);

        let blocked_by = vec!["B".to_string()];
        assert!(!has_blocked_by_cycle(
            "A",
            &blocked_by,
            resolver(&map),
            DEFAULT_MAX_DEPTH
        ));
    }

    #[test]
    fn two_node_cycle_is_detected() {
        let mut map = HashMap::new();
        map.insert("B", vec!["A"]);

        let blocked_by = vec!["B".to_string()];
        assert!(has_blocked_by_cycle(
            "A",
            &blocked_by,
            resolver(&map),
            DEFAULT_MAX_DEPTH
        ));
    }

    #[test]
    fn three_node_cycle_is_detected() {
        let mut map = HashMap::new();
        map.insert("B", vec!["C"]);
        map.insert("C", vec!["A"]);

        let blocked_by = vec!["B".to_string()];
        assert!(has_blocked_by_cycle(
            "A",
            &blocked_by,
            resolver(&map),
            DEFAULT_MAX_DEPTH
        ));
    }

    #[test]
    fn self_loop_is_detected() {
        let blocked_by = vec!["A".to_string()];
        assert!(has_blocked_by_cycle(
            "A",
            &blocked_by,
            resolver(&HashMap::new()),
            DEFAULT_MAX_DEPTH
        ));
    }

    #[test]
    fn cycle_beyond_max_depth_is_not_reached() {
        let mut map = HashMap::new();
        // Chain A → B → C → D → E → A, but max_depth = 3 cuts the walk off
        map.insert("B", vec!["C"]);
        map.insert("C", vec!["D"]);
        map.insert("D", vec!["E"]);
        map.insert("E", vec!["A"]);

        let blocked_by = vec!["B".to_string()];
        assert!(!has_blocked_by_cycle("A", &blocked_by, resolver(&map), 3));
    }

    #[test]
    fn broken_wikilink_mid_chain_is_terminal() {
        // C resolves to nothing; chain ends there with no cycle to A
        let mut map = HashMap::new();
        map.insert("B", vec!["C"]);
        // C deliberately absent

        let blocked_by = vec!["B".to_string()];
        assert!(!has_blocked_by_cycle(
            "A",
            &blocked_by,
            resolver(&map),
            DEFAULT_MAX_DEPTH
        ));
    }

    #[test]
    fn unrelated_vault_cycle_is_not_detected() {
        // B and C cycle with each other but neither leads to A.
        // Visited set prevents the walk from looping forever.
        let mut map = HashMap::new();
        map.insert("B", vec!["C"]);
        map.insert("C", vec!["B"]);

        let blocked_by = vec!["B".to_string()];
        assert!(!has_blocked_by_cycle(
            "A",
            &blocked_by,
            resolver(&map),
            DEFAULT_MAX_DEPTH
        ));
    }

    #[test]
    fn empty_blocked_by_is_never_a_cycle() {
        assert!(!has_blocked_by_cycle(
            "A",
            &[],
            resolver(&HashMap::new()),
            DEFAULT_MAX_DEPTH
        ));
    }

    #[test]
    fn multiple_blocked_by_with_one_cycling() {
        // A is blocked by B (terminal) and C (which leads back to A)
        let mut map = HashMap::new();
        map.insert("B", vec![]);
        map.insert("C", vec!["A"]);

        let blocked_by = vec!["B".to_string(), "C".to_string()];
        assert!(has_blocked_by_cycle(
            "A",
            &blocked_by,
            resolver(&map),
            DEFAULT_MAX_DEPTH
        ));
    }
}
