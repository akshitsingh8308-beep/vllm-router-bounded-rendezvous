//! Portable, dependency-free simulation of the routing decision core.
//! Run with: rustc -O tools/bounded_rendezvous_sim.rs -o bounded-sim && ./bounded-sim

#[path = "../src/policies/bounded_rendezvous_core.rs"]
mod bounded_rendezvous_core;

use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, HashMap};
use std::hash::{Hash, Hasher};

fn rank(session: &str, worker: usize) -> u64 {
    let mut hasher = DefaultHasher::new();
    session.hash(&mut hasher);
    worker.hash(&mut hasher);
    hasher.finish()
}

fn simulate(bounded: bool) -> (usize, f64, f64) {
    let mut loads = [0usize; 4];
    let mut completion: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    let mut prior: HashMap<String, usize> = HashMap::new();
    let mut remaps = 0usize;
    let mut repeats = 0usize;
    let mut peak_sum = 0usize;
    let mut max_peak = 0usize;
    for time in 0..2_000 {
        if let Some(done) = completion.remove(&time) {
            for worker in done {
                loads[worker] -= 1;
            }
        }
        let session = if time % 2 == 0 {
            "hot".to_string()
        } else {
            format!("session-{}", (time / 2) % 40)
        };
        let candidates: Vec<_> = loads
            .iter()
            .enumerate()
            .map(|(index, &load)| (index, load, rank(&session, index)))
            .collect();
        let worker = if bounded {
            bounded_rendezvous_core::choose(&candidates, 1.25).unwrap()
        } else {
            candidates.iter().max_by_key(|(_, _, r)| r).unwrap().0
        };
        if let Some(old) = prior.insert(session, worker) {
            repeats += 1;
            remaps += usize::from(old != worker);
        }
        loads[worker] += 1;
        completion.entry(time + 24).or_default().push(worker);
        let peak = *loads.iter().max().unwrap();
        max_peak = max_peak.max(peak);
        peak_sum += peak;
    }
    (max_peak, peak_sum as f64 / 2_000.0, remaps as f64 / repeats as f64)
}

fn main() {
    println!("policy,max_inflight_on_one_worker,mean_peak_inflight,repeat_session_remap_rate");
    for (name, bounded) in [("rendezvous_hash", false), ("bounded_rendezvous", true)] {
        let (max_peak, mean_peak, remap_rate) = simulate(bounded);
        println!("{name},{max_peak},{mean_peak:.3},{remap_rate:.4}");
    }
    println!("# Synthetic admissions only; no model inference or GPU latency measured.");
}
