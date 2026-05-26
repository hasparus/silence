#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
bin="${SILENCE_BIN:-$root/target/release/silence}"
runs="${RUNS:-50}"
warmup="${WARMUP:-5}"

if [[ ! -x "$bin" ]]; then
  echo "building release binary..."
  cargo build --release --manifest-path "$root/Cargo.toml" -q
  bin="$root/target/release/silence"
fi

bin_size="$(stat -f%z "$bin" 2>/dev/null || stat -c%s "$bin")"
echo "binary: $bin ($(( bin_size / 1024 )) KiB)"
echo "runs: $runs  warmup: $warmup"
echo

tmpdir="$(mktemp -d "${TMPDIR:-/tmp}/silence-bench-hook.XXXXXX")"
cleanup() { rm -rf "$tmpdir"; }
trap cleanup EXIT

cd "$tmpdir"
git init -q
git config user.email bench@test
git config user.name bench
git config commit.gpgsign false

cat > foo.ts <<'EOF'
export function hi() {
    // committed
    return "hi";
}
EOF
git add .
git commit -q -m baseline

cat > foo.ts <<'EOF'
export function hi() {
    // committed
    return "hi";
    // agent slop
}
EOF

stdin_json="$(python3 -c 'import json, pathlib; print(json.dumps({"hook_event_name":"PostToolUse","tool_name":"Edit","tool_input":{"file_path":str(pathlib.Path("foo.ts").resolve())}}))')"

measure() {
  local label=$1
  shift
  python3 - "$label" "$bin" "$runs" "$warmup" "$@" <<'PY'
import subprocess
import sys
import time

label, bin_path, runs_s, warmup_s, *cmd = sys.argv[1:]
runs = int(runs_s)
warmup = int(warmup_s)
full = [bin_path, *cmd]

for _ in range(warmup):
    subprocess.run(full, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False)

samples = []
for _ in range(runs):
    t0 = time.perf_counter()
    r = subprocess.run(full, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True)
    t1 = time.perf_counter()
    if r.returncode != 0:
        sys.stderr.write(r.stderr)
        raise SystemExit(f"{label}: exit {r.returncode}")
    samples.append((t1 - t0) * 1000)

samples.sort()
n = len(samples)

def pct(p):
    if n == 1:
        return samples[0]
    i = (n - 1) * p / 100
    lo = int(i)
    hi = min(lo + 1, n - 1)
    w = i - lo
    return samples[lo] * (1 - w) + samples[hi] * w

print(f"{label}")
print(f"  min {samples[0]:.2f} ms")
print(f"  p50 {pct(50):.2f} ms")
print(f"  p95 {pct(95):.2f} ms")
print(f"  max {samples[-1]:.2f} ms")
print(f"  avg {sum(samples) / n:.2f} ms")
print()
PY
}

measure "hook explicit path (foo.ts)" hook foo.ts
printf '%s' "$stdin_json" | measure "hook stdin (Edit file_path)" hook

git checkout -q foo.ts
measure "hook skip (no uncommitted change)" hook foo.ts

echo "tip: RUNS=200 WARMUP=20 SILENCE_BIN=./target/release/silence $0"
