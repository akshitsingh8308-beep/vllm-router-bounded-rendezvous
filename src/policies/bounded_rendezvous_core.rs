//! Dependency-free selection core for bounded-load rendezvous routing.
//!
//! Each tuple is (worker index, in-flight load, rendezvous rank). The caller
//! filters unhealthy workers and supplies ranks for its chosen session key.

pub fn choose(candidates: &[(usize, usize, u64)], load_factor: f64) -> Option<usize> {
    assert!(load_factor.is_finite() && load_factor >= 1.0);
    if candidates.is_empty() {
        return None;
    }
    let total = candidates
        .iter()
        .fold(0usize, |sum, (_, load, _)| sum.saturating_add(*load));
    let cap = (((total.saturating_add(1) as f64 * load_factor) / candidates.len() as f64).ceil()
        as usize)
        .max(1);
    candidates
        .iter()
        .filter(|(_, load, _)| *load < cap)
        .max_by_key(|(_, _, rank)| *rank)
        .or_else(|| {
            candidates
                .iter()
                .min_by(|a, b| a.1.cmp(&b.1).then_with(|| b.2.cmp(&a.2)))
        })
        .map(|(index, _, _)| *index)
}

#[cfg(test)]
mod tests {
    use super::choose;

    #[test]
    fn chooses_highest_rank_when_idle() {
        assert_eq!(choose(&[(0, 0, 10), (1, 0, 20), (2, 0, 15)], 1.25), Some(1));
    }

    #[test]
    fn diverts_when_preference_is_overloaded() {
        assert_eq!(choose(&[(0, 8, 30), (1, 0, 20), (2, 0, 10)], 1.25), Some(1));
    }

    #[test]
    fn excludes_worker_at_cap() {
        assert_eq!(choose(&[(0, 3, 30), (1, 2, 20), (2, 1, 15)], 1.0), Some(1));
    }

    #[test]
    fn empty_and_single_worker() {
        assert_eq!(choose(&[], 1.25), None);
        assert_eq!(choose(&[(7, 99, 1)], 1.25), Some(7));
    }
}
