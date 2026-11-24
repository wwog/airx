#!/usr/bin/env bash
# 一条命令刷新 reference/gemini：文档快照 + SDK 裁剪 + API 清单
#
#   ./update.sh                # 三步全做
#   ./update.sh --docs-only    # 只抓文档
#   ./update.sh --sdk-only     # 只拉 SDK 并重建 API 清单
#
# 环境变量：SDK_VERSION=3.6.1 / SDK_FORCE=1 / DOCS_SLEEP=0.3 ...
# 幂等：同名快照覆盖；SDK 版本未变且已完整时跳过下载，输出不变。
set -uo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./versions.env
. "${here}/versions.env"

mode="all"
case "${1:-}" in
  "")
    ;;
  --docs-only) mode="docs" ;;
  --sdk-only)  mode="sdk" ;;
  -h|--help)
    sed -n '2,10p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
    exit 0
    ;;
  *)
    echo "未知参数：$1（可用：--docs-only / --sdk-only）" >&2
    exit 2
    ;;
esac

say() { printf '%s\n' "$*"; }
failed=()

if [ "${mode}" = "all" ] || [ "${mode}" = "docs" ]; then
  say "==> [1/3] docs"
  if ! bash "${here}/scripts/fetch-docs.sh"; then
    failed+=("docs")
  fi
else
  say "==> [1/3] docs (跳过)"
fi

if [ "${mode}" = "all" ] || [ "${mode}" = "sdk" ]; then
  say "==> [2/3] sdk"
  if ! bash "${here}/scripts/fetch-sdk.sh"; then
    failed+=("sdk")
  fi

  sdk_version=""
  [ -s "${here}/_tmp/sdk-version.txt" ] && sdk_version="$(tr -d '[:space:]' < "${here}/_tmp/sdk-version.txt")"
  if [ -z "${sdk_version}" ]; then
    latest_dir="$(ls -d "${here}"/sdk/@google-genai@* 2>/dev/null | sort -V | tail -1)"
    [ -n "${latest_dir}" ] && sdk_version="${latest_dir##*@}"
  fi

  sdk_dir="${here}/sdk/@google-genai@${sdk_version}"
  say "==> [3/3] api（${sdk_dir#${here}/}）"
  if [ -z "${sdk_version}" ] || [ ! -f "${sdk_dir}/dist/genai.d.ts" ]; then
    say "错误：找不到 SDK 目录 ${sdk_dir}，跳过 API 清单生成" >&2
    failed+=("api")
  elif ! node "${here}/scripts/extract-api.mjs" "${sdk_dir}" "${here}/api"; then
    failed+=("api")
  elif ! node -e '
    // 与 openai / anthropic 共享的清单契约校验：
    //   coverage.params_types === types.length（types[] 只放请求参数类型）
    //   coverage.other_types === other_types.length
    //   coverage.types_total === params_types + other_types
    const fs = require("fs");
    const m = JSON.parse(fs.readFileSync(process.argv[1], "utf8"));
    const c = m.coverage || {};
    const problems = [];
    if (c.params_types !== m.types.length) problems.push("params_types != types.length");
    if (c.other_types !== (m.other_types || []).length) problems.push("other_types != other_types.length");
    if (c.types_total !== m.types.length + (m.other_types || []).length) problems.push("types_total != params_types + other_types");
    if (c.params_types > c.types_total) problems.push("params_types > types_total");
    if (c.methods !== m.methods.length) problems.push("methods != methods.length");
    if (c.resources !== new Set(m.methods.map((x) => x.resource)).size) problems.push("resources != distinct resources");
    if (new Set(m.methods.map((x) => x.resource + "." + x.method)).size !== m.methods.length) problems.push("duplicate resource.method");
    if (problems.length) { console.error("coverage 校验失败: " + problems.join("; ")); process.exit(1); }
  ' "${here}/api/manifest.json"; then
    failed+=("api-check")
  fi
else
  say "==> [2/3] sdk (跳过)"
  say "==> [3/3] api (跳过)"
fi

say ""
if [ "${#failed[@]}" -gt 0 ]; then
  say "完成，但以下步骤失败：${failed[*]}" >&2
  exit 1
fi
case "${mode}" in
  docs) say "完成：docs/ 已刷新（sdk/、api/ 未改动）。" ;;
  sdk)  say "完成：sdk/、api/ 已刷新（docs/ 未改动）。" ;;
  *)    say "完成：docs/、sdk/、api/ 已刷新。" ;;
esac
