#!/usr/bin/env bash
# 校验 crate 依赖方向，规则见 docs/architecture.md §4.3。
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

meta="$(mktemp)"
trap 'rm -f "$meta"' EXIT

cargo metadata --no-deps --format-version 1 >"$meta"

python3 - "$meta" <<'PY'
import json
import sys

# 每个 crate 允许直接依赖的 workspace crate
ALLOWED = {
    "airx-core": set(),
    "airx-protocol": {"airx-core"},
    "airx-transport": {"airx-core"},
    "airx-unified": {"airx-core", "airx-protocol"},
    "airx": {"airx-core", "airx-protocol", "airx-transport", "airx-unified"},
}

with open(sys.argv[1], encoding="utf-8") as fh:
    meta = json.load(fh)

problems = []
for pkg in meta["packages"]:
    name = pkg["name"]
    if name not in ALLOWED:
        continue
    for dep in pkg["dependencies"]:
        # 只校验普通依赖；dev-dependencies 允许为测试构成环
        if dep.get("kind") is not None:
            continue
        dep_name = dep["name"]
        if dep_name == name:
            continue
        if dep_name in ALLOWED and dep_name not in ALLOWED[name]:
            problems.append(f"{name} -> {dep_name}")

if problems:
    print("非法的 workspace 依赖方向（见 docs/architecture.md §4.3）：", file=sys.stderr)
    for problem in problems:
        print(f"  {problem}", file=sys.stderr)
    sys.exit(1)

print("依赖方向检查通过：5 个 crate，单向无环")
PY
