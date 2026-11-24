#!/usr/bin/env bash
# 依次刷新三家官方文档与 SDK API 清单。
# 单个 provider 失败不影响其余；最后汇总退出码。
set -uo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
providers=(openai anthropic gemini)
failed=()

for provider in "${providers[@]}"; do
  echo "==> ${provider}"
  if ! "${root}/${provider}/update.sh" "$@"; then
    failed+=("${provider}")
  fi
done

if ((${#failed[@]})); then
  echo "失败：${failed[*]}" >&2
  exit 1
fi

echo "全部完成。"
