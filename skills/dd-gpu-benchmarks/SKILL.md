---
name: dd-gpu-benchmarks
description: GPU Monitoring inference benchmarks - load-test a model endpoint, wait for runs, and compare results.
metadata:
  version: "1.0.0"
  author: datadog-labs
  repository: https://github.com/datadog-labs/agent-skills
  tags: datadog,gpu,inference,benchmarks,llm,vllm,sglang,dynamo,dd-gpu-benchmarks
  alwaysApply: "false"
---

# Datadog GPU Inference Benchmarks

Load-test an OpenAI-compatible inference endpoint with AIPerf, from Datadog's benchmark cluster or your own Kubernetes cluster, and read the results. Use this skill to answer "how fast is my endpoint?", "did this change make it faster?", and "how many concurrent users can it serve within my latency targets?".

## Prerequisites
This requires the pup binary in your path.

`pup` - `cargo install --git https://github.com/DataDog/pup`

```bash
pup auth login --extra-scopes gpu_benchmarks_read,gpu_benchmarks_write,gpu_benchmarks_run
```

`gpu_benchmarks_read` reads benchmarks and runs, `gpu_benchmarks_write` creates and edits them, and `gpu_benchmarks_run` starts and cancels runs and sweeps.

## Before you start a run

Runs and sweeps send real traffic to the endpoint, and can load a production model server for up to the benchmark's `max_duration_seconds` (10 minutes by default). Before `runs start` or `sweeps start`:

- Confirm with the user which endpoint will be loaded, and for how long.
- Pass an `--idempotency-key`. If the command fails or times out, retry with the same key: it returns the run it already started instead of starting a second one.
- One run per benchmark at a time: starting a run while another is active fails with HTTP 409.

## Find existing benchmarks

```bash
pup gpu benchmarks list
pup gpu benchmarks list --deployment-name llama-70b-vllm --deployment-env prod
pup gpu benchmarks get <benchmark-id>
pup gpu benchmarks runs list <benchmark-id>
```

## Create a benchmark

Check the endpoint first. This sends one chat completion and saves nothing:

```bash
cat > endpoint.json <<'EOF'
{"url": "https://llm-gateway.example.com/v1", "model": "meta-llama/Llama-3.1-70B-Instruct", "streaming": true,
 "headers": [{"name": "Authorization", "value": "Bearer <token>"}]}
EOF
pup gpu benchmarks test-connection --file endpoint.json
```

Then create the benchmark. `endpoint` is required; everything else has defaults:

```bash
cat > benchmark.json <<'EOF'
{
  "name": "llama-70b chat, 32 users",
  "endpoint": {"url": "https://llm-gateway.example.com/v1", "model": "meta-llama/Llama-3.1-70B-Instruct", "streaming": true,
               "headers": [{"name": "Authorization", "value": "Bearer <token>"}]},
  "workload": {"concurrency": 32, "request_count": 1000, "warmup_request_count": 32,
               "input_tokens": {"mean": 1024, "stddev": 0}, "output_tokens": {"mean": 256, "stddev": 0}},
  "targets": {"ttft_ms": 500, "itl_ms": 50},
  "deployment": {"source": "vllm", "tag_key": "service", "name": "llama-70b-vllm", "env": "prod"}
}
EOF
pup gpu benchmarks create --file benchmark.json
```

- Header values are write-only: responses list header names only.
- `location` is `{"type": "datadog"}` by default. Use `{"type": "cluster", "orch_cluster_id": "<id>"}` to run from your own cluster, for endpoints that are not reachable from the internet.
- `deployment` links the benchmark to a model deployment of the GPU Monitoring Inference page, so its runs show there.
- `update` replaces `workload`, `targets` and `deployment` whole. Changing the workload makes later runs not comparable with earlier ones.

## Run and wait

```bash
pup gpu benchmarks runs start <benchmark-id> --idempotency-key vllm-0.6.3-upgrade
pup gpu benchmarks runs wait <run-id> --timeout 30m
```

`wait` returns once the run is `completed`, `failed`, `invalid`, `timeout` or `cancelled`. Check `status` and `reason` before reading results.

## Read results

`runs get` and `runs wait` return the run with its `summary` once results are recorded:

- `ttft_ms` and `itl_ms`: time to first token and inter-token latency, with `p50`, `p90`, `p99`, `mean` and `std`, in milliseconds.
- `successful_request_count`: the requests behind the means and standard deviations.
- `output_token_throughput`, `total_token_throughput` (tokens/s) and `request_throughput` (requests/s).
- `good_request_fraction`: the share of requests within the benchmark's targets. It is absent without targets.

The run also has snapshots of what it ran: `endpoint`, `workload`, `targets`, `deployment`, and `deployment_config` (engine, version, GPUs, parallelism, KV cache) captured when it started and ended.

## Compare two runs

1. Check that they are comparable: the same `endpoint`, `workload` and `targets` in both snapshots. If `deployment_config` differs, that difference is what you are measuring (for example a new engine version).
2. For each metric, compute the relative change: `(new - old) / old`. For latencies lower is better; for throughputs higher is better.
3. Before calling a latency change a regression or an improvement, check that it is beyond noise. Use Welch's t-test at 95% on `mean`, `std` and `successful_request_count`:
   - `se = sqrt(std_a² / n_a + std_b² / n_b)` and `t = |mean_a - mean_b| / se`.
   - The difference is significant when `t > 1.96` (with more than about 100 requests per run).
   - Percentile moves without a significant mean change are usually noise.

## Find the highest load within targets

A sweep runs the benchmark once per concurrency level. It needs the benchmark to have `targets`.

```bash
# The highest concurrency where at least 95% of requests meet the targets
pup gpu benchmarks sweeps start <benchmark-id> --kind search --max-concurrency 64 --idempotency-key capacity-1
# Throughput and latency at fixed levels
pup gpu benchmarks sweeps start <benchmark-id> --kind curve --concurrency-levels 1,4,16,32,64

pup gpu benchmarks sweeps get <sweep-id>
```

`sweeps get` returns `steps` (`concurrency`, `status`, `passed`, `summary` for each) and the `result`: `found` with the `concurrency`, `reached_max` when every level passed, or `none` when even the lowest failed. A sweep runs its steps one after another, so it takes several run durations. Poll `sweeps get` rather than waiting on each run.

## Errors

| HTTP | Meaning |
|------|---------|
| 403 | Missing `gpu_benchmarks_*` permission or OAuth scope. Re-run `pup auth login` with the extra scopes above. |
| 404 | Not found in this org, or not available on this site. |
| 409 | A run or sweep is already active on the benchmark, or it cannot be deleted while one is active. |
| 422 | The definition is invalid; the error names the field. |
| 429 | Over the org's run quota or the API rate limit. |
