## Forbidden Fixes

These patterns are **always invalid**, regardless of category. A fix that only reduces flake probability instead of eliminating the root cause is not a fix.

- **Blind timing hacks**: adding `time.Sleep`, or widening timeouts/retries when the root cause is not the timeout itself (e.g. to paper over a race condition) — these mask the problem and slow the test suite. Exception: increasing a timeout IS valid when the `timeout` category confirms the deadline is genuinely too tight for the operation under test.
- **Relaxed assertions**: changing exact matches to `at_least(1)`, `contains`, or range checks when the exact value is deterministic and should be asserted precisely
- **Wrong-layer patches**: fixing a test when the bug is in production code, or touching production code for a test-only issue
- **Partial fixes**: fixing one callsite of a shared helper when multiple callsites share the root cause — find and fix all callsites
- **Infrastructure workarounds**: any code change that attempts to work around Docker, network, or CI runner unavailability — these cannot be fixed in test code

---

## Flaky Test Fix Patterns by Category

For each `flaky_category` value, apply the corresponding fix pattern. Use stack trace evidence from STEP 2 to confirm the pattern before proposing a change.

---

### timeout

**Signal:** Test exceeds time limit; stack trace shows timeout or deadline exceeded.

**Confirm first:** Verify the deadline itself is the root cause — not an underlying race, stuck goroutine, or async ordering issue that happens to surface as a timeout. If `concurrency` or `asynchronous_wait` signals are present, fix the synchronization instead.

**Preferred fix:** Replace real waits with mocked/injectable time so the test runs deterministically regardless of environment speed.

```go
type Clock interface { Now() time.Time }
// inject a fake clock in tests instead of relying on wall time
```

**Acceptable fix:** Increase the timeout when the deadline is provably too tight for the operation (e.g. a real I/O call that consistently needs more time than allowed). Increase proportionally — 2–5× the current value — not arbitrarily.

```go
// Before — too tight for the operation
ctx, cancel := context.WithTimeout(ctx, 1*time.Second)

// After — proportional increase with justification
ctx, cancel := context.WithTimeout(ctx, 5*time.Second)
```

**Invalid:** Bumping a timeout to make a race condition or async ordering issue fail less often — this masks the real problem.

For polling loops, add a maximum retry count with exponential backoff rather than a fixed sleep.

---

### concurrency

**Signal:** Data race, deadlock, or non-deterministic ordering. Stack trace may show goroutine contention.

**Fix:** Add synchronization. Common patterns:

```go
// Add mutex around shared state
var mu sync.Mutex
mu.Lock()
sharedState = value
mu.Unlock()

// Use sync.WaitGroup for goroutine coordination
var wg sync.WaitGroup
wg.Add(1)
go func() { defer wg.Done(); doWork() }()
wg.Wait()
```

---

### time

**Signal:** Assertions compare timestamps or durations that vary by wall-clock time.

**Fix:** Inject a clock interface; use a fake clock in tests.

```go
// Before — uses time.Now() directly
func IsExpired(t time.Time) bool { return time.Now().After(t) }

// After — injectable clock
type Clock interface { Now() time.Time }
func IsExpired(t time.Time, clk Clock) bool { return clk.Now().After(t) }
```

In tests, pass a `fakeClock` that returns a deterministic value.

---

### network

**Signal:** Connection refused, DNS failure, or timeout on external calls.

**Fix:** Mock the HTTP client or network dependency in tests.

```go
// Inject an http.Client so tests can swap it
server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
    w.Write([]byte(`{"ok":true}`))
}))
defer server.Close()
client := &Client{httpClient: server.Client()}
```

---

### randomness

**Signal:** Test output varies between runs with no environmental change.

**Fix:** Seed the random number generator with a fixed value in tests.

```go
// Before
r := rand.Int()

// After — deterministic in tests
rng := rand.New(rand.NewSource(42))
r := rng.Int()
```

---

### order_dependency

**Signal:** Test passes in isolation but fails when run with other tests; order of test execution matters.

**Fix:** Add proper setup and teardown to isolate test state.

```go
func TestMyFeature(t *testing.T) {
    // Reset shared state before test
    db.Reset()
    cache.Clear()
    defer func() {
        db.Reset()
    }()
    // ... test body
}
```

---

### environment_dependency

**Signal:** Test fails only in CI or only on certain machines; depends on env vars or installed tools.

**Fix:** Inject configuration rather than reading env vars directly in the function under test.

```go
// Before — reads env var in tested function
func NewClient() *Client { return &Client{endpoint: os.Getenv("API_URL")} }

// After — injectable config
type Config struct { APIEndpoint string }
func NewClient(cfg Config) *Client { return &Client{endpoint: cfg.APIEndpoint} }
```

---

### io

**Signal:** File not found, permission denied, or leftover files from previous runs.

**Fix:** Use `t.TempDir()` for test files; it is cleaned up automatically after the test.

```go
func TestFileProcessing(t *testing.T) {
    dir := t.TempDir()
    path := filepath.Join(dir, "test.txt")
    os.WriteFile(path, []byte("content"), 0644)
    // ... test using path
}
```

---

### resource_leak

**Signal:** Resource exhaustion (open files, goroutines, DB connections) accumulates across tests.

**Fix:** Use `defer` to release resources; use `t.Cleanup` for test-scoped cleanup.

```go
conn, err := db.Connect()
require.NoError(t, err)
t.Cleanup(func() { conn.Close() })
```

---

### too_restrictive_range

**Signal:** Assertion checks for an exact value that legitimately varies (latency, size, timestamp).

**Fix:** Widen the assertion to a range or relative comparison.

```go
// Before — exact match on timing
assert.Equal(t, 100*time.Millisecond, elapsed)

// After — allow variance
assert.InDelta(t, 100.0, elapsed.Milliseconds(), 50.0) // ±50ms
```

---

### unordered_collection

**Signal:** Assertion fails because slice or map iteration order varies between runs.

**Fix:** Sort before comparing, or use a set-based assertion.

```go
// Before
assert.Equal(t, []string{"a", "b", "c"}, result)

// After — sort both sides, or use ElementsMatch
sort.Strings(result)
assert.Equal(t, []string{"a", "b", "c"}, result)

// Or with testify
assert.ElementsMatch(t, []string{"a", "b", "c"}, result)
```

---

### floating_point

**Signal:** Floating-point equality assertion fails due to precision differences.

**Fix:** Use tolerance-based comparison.

```go
// Before
assert.Equal(t, 0.3, 0.1+0.2)

// After
assert.InDelta(t, 0.3, 0.1+0.2, 1e-9)
```

---

### asynchronous_wait

**Signal:** Test races against an async operation; result is checked before it completes.

**Fix:** Use channels, `sync.WaitGroup`, or a polling helper with a deadline instead of fixed sleeps.

```go
// Before — fixed sleep, races against async completion
time.Sleep(100 * time.Millisecond)
assert.Equal(t, "done", getStatus())

// After — poll with a deadline
require.Eventually(t, func() bool {
    return getStatus() == "done"
}, 5*time.Second, 50*time.Millisecond)
```

---

### unknown

Category could not be determined automatically. **Do not attempt a code fix.** Follow STEP 4/5 in the main skill: self-introduced flakes escalate to the author; pre-existing or undetermined flakes follow the impact thresholds and infrastructure guard. Include the full triage brief for manual investigation.
