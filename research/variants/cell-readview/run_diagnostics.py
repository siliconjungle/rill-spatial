#!/usr/bin/env python3
"""Sequential bounded-output diagnostics, never cross-variant acceptance timings."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess

p = argparse.ArgumentParser()
p.add_argument("--fixtures", type=Path, required=True)
p.add_argument("--stride", type=int, default=1)
p.add_argument("--repetitions", type=int, default=1)
p.add_argument("--output", type=Path, required=True)
args = p.parse_args()
root = Path(__file__).resolve().parent
binary = root / ".build/release/examples/stream_probe"
sha = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
sources = {str(path.relative_to(root)): sha(path) for path in [root / "src/lib.rs", root / "Cargo.toml", root / "Cargo.lock", root / "examples/stream_probe.rs"]}
records = []
for fixture in sorted(args.fixtures.resolve().glob("*.bin")):
    digest = sha(fixture)
    for strategy in ["hybrid", "hash", "sorted", "occupied"]:
        env = dict(os.environ, RILL_CELL_STRATEGY=strategy)
        command = ["/usr/bin/time", "-l", str(binary), str(fixture), str(args.repetitions), str(args.stride)]
        result = subprocess.run(command, env=env, text=True, capture_output=True)
        if result.returncode:
            raise RuntimeError(result.stderr)
        rows = [json.loads(line) for line in result.stdout.splitlines()]
        records.append({"fixture": str(fixture), "sha256": digest, "strategy": strategy, "status": "exploratory_concurrent_not_acceptance", "command": command, "source_sha256": sources, "binary_sha256": sha(binary), "results": rows, "stderr": result.stderr})
        args.output.write_text(json.dumps(records, indent=2) + "\n")
        print(fixture.stem, strategy, rows[-1]["total_ns"], flush=True)
