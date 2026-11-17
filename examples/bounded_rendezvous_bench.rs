//! Deterministic admission simulation using the real router policy implementations.
//! Run: cargo run --release --example bounded_rendezvous_bench

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use vllm_router_rs::core::{BasicWorker, Worker, WorkerType};
use vllm_router_rs::policies::{
    BoundedRendezvousPolicy, LoadBalancingPolicy, RendezvousHashPolicy,
};

fn run(policy: &dyn LoadBalancingPolicy) -> serde_json::Value {
    let workers: Vec<Arc<dyn Worker>> = (0..4)
        .map(|i| {
            Arc::new(BasicWorker::new(
                format!("http://worker-{i}:8000"),
                WorkerType::Regular,
            )) as Arc<dyn Worker>
        })
        .collect();
    let mut completion: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    let mut previous: HashMap<String, usize> = HashMap::new();
    let mut remaps = 0usize;
    let mut repeats = 0usize;
    let mut max_load = 0usize;
    let mut sum_peak = 0usize;
    let requests = 2_000usize;

    for t in 0..requests {
        if let Some(finished) = completion.remove(&t) {
            for i in finished {
                workers[i].decrement_load();
            }
        }
        // One hot session accounts for half of traffic. Other sessions recur.
        let session = if t % 2 == 0 {
            "hot".to_string()
        } else {
            format!("session-{}", (t / 2) % 40)
        };
        let body = format!(r#"{{"session_id":"{session}"}}"#);
        let i = policy
            .select_worker(&workers, Some(&body))
            .expect("healthy worker");
        if let Some(old) = previous.insert(session, i) {
            repeats += 1;
            remaps += usize::from(old != i);
        }
        workers[i].increment_load();
        completion.entry(t + 24).or_default().push(i);
        let peak = workers.iter().map(|w| w.load()).max().unwrap();
        max_load = max_load.max(peak);
        sum_peak += peak;
    }
    serde_json::json!({
        "policy": policy.name(),
        "requests": requests,
        "workers": workers.len(),
        "service_steps": 24,
        "max_inflight_on_one_worker": max_load,
        "mean_peak_inflight": sum_peak as f64 / requests as f64,
        "repeat_session_remap_rate": remaps as f64 / repeats as f64,
        "note": "Deterministic policy simulation; not a measured GPU latency result"
    })
}

fn main() {
    let baseline = RendezvousHashPolicy::new();
    let candidate = BoundedRendezvousPolicy::new(1.25);
    println!("{}", serde_json::to_string_pretty(&run(&baseline)).unwrap());
    println!(
        "{}",
        serde_json::to_string_pretty(&run(&candidate)).unwrap()
    );
}
