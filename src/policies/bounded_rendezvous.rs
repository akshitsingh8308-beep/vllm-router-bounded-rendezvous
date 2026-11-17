//! Bounded-load rendezvous routing.
//!
//! Rendezvous hashing gives a stable preferred worker for a session. This
//! policy keeps that preference while it has spare capacity, and chooses the
//! highest-ranked worker below a soft in-flight load cap otherwise.

use std::sync::Arc;

use super::bounded_rendezvous_core;
use super::{
    get_healthy_worker_indices, hash_key, ConsistentHashPolicy, LoadBalancingPolicy, RequestHeaders,
};
use crate::core::Worker;
use crate::metrics::RouterMetrics;

/// Maximum allowed load is `ceil((total_load + 1) * load_factor / workers)`.
/// The cap is at least one, so an idle cluster can admit requests.
#[derive(Debug)]
pub struct BoundedRendezvousPolicy {
    load_factor: f64,
}

impl BoundedRendezvousPolicy {
    pub fn new(load_factor: f64) -> Self {
        assert!(load_factor.is_finite() && load_factor >= 1.0);
        Self { load_factor }
    }

    fn choose(&self, workers: &[Arc<dyn Worker>], key: &str) -> Option<usize> {
        let healthy = get_healthy_worker_indices(workers);
        let candidates: Vec<_> = healthy
            .iter()
            .map(|&i| {
                (
                    i,
                    workers[i].load(),
                    ConsistentHashPolicy::fbi_hash(&format!("{}:{}", key, workers[i].url())),
                )
            })
            .collect();
        bounded_rendezvous_core::choose(&candidates, self.load_factor)
    }
}

impl LoadBalancingPolicy for BoundedRendezvousPolicy {
    fn select_worker_with_headers(
        &self,
        workers: &[Arc<dyn Worker>],
        request_text: Option<&str>,
        headers: Option<&RequestHeaders>,
    ) -> Option<usize> {
        let key = hash_key::extract_hash_key(request_text, headers);
        let idx = self.choose(workers, &key)?;
        workers[idx].increment_processed();
        RouterMetrics::record_processed_request(workers[idx].url());
        RouterMetrics::record_policy_decision(self.name(), workers[idx].url());
        Some(idx)
    }

    fn name(&self) -> &'static str {
        "bounded_rendezvous"
    }
    fn needs_request_text(&self) -> bool {
        true
    }
    fn needs_headers(&self) -> bool {
        true
    }
    fn tracks_request_load(&self) -> bool {
        true
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(all(test, feature = "cpu", feature = "ready"))]
mod tests {
    use super::*;
    use crate::core::{BasicWorker, WorkerType};

    fn workers() -> Vec<Arc<dyn Worker>> {
        (0..3)
            .map(|i| {
                Arc::new(BasicWorker::new(
                    format!("http://worker{i}:8000"),
                    WorkerType::Regular,
                )) as Arc<dyn Worker>
            })
            .collect()
    }

    #[test]
    fn stable_when_below_cap() {
        let policy = BoundedRendezvousPolicy::new(1.25);
        let workers = workers();
        let first = policy.select_worker(&workers, Some(r#"{"session_id":"s"}"#));
        for _ in 0..10 {
            assert_eq!(
                policy.select_worker(&workers, Some(r#"{"session_id":"s"}"#)),
                first
            );
        }
    }

    #[test]
    fn diverts_from_overloaded_preference() {
        let policy = BoundedRendezvousPolicy::new(1.25);
        let workers = workers();
        let request = r#"{"session_id":"s"}"#;
        let preferred = policy.select_worker(&workers, Some(request)).unwrap();
        for _ in 0..10 {
            workers[preferred].increment_load();
        }
        let chosen = policy.select_worker(&workers, Some(request)).unwrap();
        assert_ne!(chosen, preferred);
        assert_eq!(workers[chosen].load(), 0);
    }

    #[test]
    fn ignores_unhealthy_and_handles_empty_pool() {
        let policy = BoundedRendezvousPolicy::new(1.25);
        assert_eq!(policy.select_worker(&[], None), None);
        let workers = workers();
        workers[0].set_healthy(false);
        for i in 0..20 {
            let request = format!(r#"{{"session_id":"{i}"}}"#);
            assert_ne!(policy.select_worker(&workers, Some(&request)), Some(0));
        }
    }
}
