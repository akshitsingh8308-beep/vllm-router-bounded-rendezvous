# Bounded-load rendezvous routing for vLLM Router

This fork adds `bounded_rendezvous`, a session-aware routing policy for the
open-source vLLM Router. It ranks healthy workers with rendezvous hashing and
admits a request to the highest-ranked worker whose current in-flight load is
below a soft cap. The goal is to preserve session locality while preventing a
hot session from concentrating all new requests on one worker.

## What was built

- A Rust policy integrated with the router's CLI, config, policy factory, and
  model policy registry.
- A configurable `--bounded-load-factor` (default `1.25`, minimum `1.0`).
- Unit tests for stable placement, overload diversion, and unhealthy workers.
- A deterministic simulation that executes the real Rust policies and compares
  maximum per-worker in-flight load and repeat-session remapping.

## Run

Install the toolchain and dependencies described in the upstream [README](README.md).
From this folder:

```bash
cargo test --features cpu,ready bounded_rendezvous
cargo run --release --example bounded_rendezvous_bench
cargo run --release -- --policy bounded_rendezvous \
  --bounded-load-factor 1.25 \
  --worker-urls http://127.0.0.1:8000 http://127.0.0.1:8001
```

The two worker URLs must point to running OpenAI-compatible inference servers
for live requests. The policy and simulation run without a GPU; live inference
requires workers.

## Policy

For the healthy worker pool, let `L` be total in-flight requests, `N` be the
number of workers, and `c` be the load factor. The admission cap is:

```text
max(1, ceil(c * (L + 1) / N))
```

Workers below the cap are eligible. Among them, the policy chooses the highest
rendezvous hash of the session key and worker URL. If every worker is at the
cap, it chooses the least loaded healthy worker, breaking ties by hash rank.
The router's selection context reserves the chosen worker's local load before
another admission through that context.

Session affinity is best effort. It can change while the preferred worker is
at or above the cap, during health changes, and when the worker set changes.
This is intentional: keeping every request of a hot session on one overloaded
worker would defeat the load bound. Load is the router's local in-flight count,
not a direct measure of GPU KV cache occupancy or queue time.

## Evaluation

`examples/bounded_rendezvous_bench.rs` admits 2,000 requests to four mock
workers. Half the traffic repeats one hot session; the other half rotates
across 40 sessions. Each request stays in flight for 24 simulated admission
steps. It compares this policy with the existing `rendezvous_hash` policy.
The output reports peak in-flight load and session remapping. These are
**simulation metrics**, not measured time to first token, real GPU throughput,
or a production trace.

The dependency-free core can also be checked on Windows without building the
upstream vLLM dependencies:

```bash
rustc --test src/policies/bounded_rendezvous_core.rs -o bounded-core-tests
./bounded-core-tests
rustc -O tools/bounded_rendezvous_sim.rs -o bounded-sim
./bounded-sim
```

In the local synthetic run, maximum in-flight requests on one worker fell
from **18 to 8**; repeat-session remapping rose from **0% to 34.1%**. The
simulation's hash function differs from the router's production hash. The
Linux example above runs the actual router policies and should be preferred
for a final published benchmark.

For a stronger evaluation, run the router against at least two real vLLM
workers with prefix caching enabled and replay a shared-prefix workload.
Measure p50/p90 time to first token, cache hit rate, per-worker queue depth,
throughput, and router overhead. Results should be reported with hardware,
model, workload, warm-up, and repetitions.

## Resume wording

After reproducing the results, a defensible description is: "Extended
vLLM Router (Rust) with bounded-load rendezvous routing, adding session-aware
placement, CLI configuration, tests, and a reproducible benchmark."

Only add numeric performance claims after real-engine measurements.

## Verification status

On Windows, the dependency-free core passed four unit tests and the portable
simulation ran. The full router build was attempted but stopped in upstream
`vllm-engine-core-client`, which uses `tokio::net::UnixStream` and requires a
Unix target. `.github/workflows/bounded-rendezvous.yml` runs the full policy
tests and actual-policy benchmark on Linux after the repository is pushed.

## Provenance

Based on `vllm-project/router` at commit `2b62a3d` (2026-11-16). See the
upstream [license](LICENSE) and the fork's Git history for exact changes.
