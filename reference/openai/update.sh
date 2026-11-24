#!/usr/bin/env bash
# 一条命令刷新 OpenAI 快照：官方文档 + npm SDK 裁剪 + API 清单。
#
#   用法:
#     ./update.sh                   # docs + sdk + api 全刷
#     ./update.sh --docs-only       # 只抓文档
#     ./update.sh --sdk-only        # 只拉 SDK 并重建 API 清单
#     ./update.sh --best-effort     # 个别文档页失败时保留部分快照（失败清单进 meta.json）
#     SDK_VERSION=7.63.0 ./update.sh
#
# 幂等：重复执行只覆盖同名快照；内容未变的文档不重写，SDK 版本未变时 sdk/ 目录内容不变。
set -euo pipefail

provider_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./versions.env
. "${provider_dir}/versions.env"

mode="all"
extra_args=()

usage() {
  sed -n '2,12p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --docs-only) mode="docs" ;;
    --sdk-only) mode="sdk" ;;
    --best-effort|--strict) extra_args+=("$1") ;;
    -h|--help) usage; exit 0 ;;
    *)
      echo "update.sh: 未知参数 $1" >&2
      usage >&2
      exit 2
      ;;
  esac
  shift
done

log() { printf '%s\n' "$*" >&2; }

mkdir -p "${provider_dir}/docs" "${provider_dir}/sdk" "${provider_dir}/api" \
  "${provider_dir}/raw" "${provider_dir}/_tmp"

started_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
log "openai 快照刷新开始：${started_at}（mode=${mode}）"

if [ "${mode}" = "all" ] || [ "${mode}" = "docs" ]; then
  log ""
  log "### [1/3] 文档"
  "${provider_dir}/scripts/fetch-docs.sh" ${extra_args[@]+"${extra_args[@]}"}
else
  log ""
  log "### [1/3] 文档：跳过（--sdk-only）"
fi

if [ "${mode}" = "all" ] || [ "${mode}" = "sdk" ]; then
  log ""
  log "### [2/3] SDK"
  "${provider_dir}/scripts/fetch-sdk.sh"

  # fetch-sdk.sh 把解析后的目录与版本写到 raw/sdk/latest.tsv
  sdk_info="${provider_dir}/raw/sdk/latest.tsv"
  [ -f "${sdk_info}" ] || { log "缺少 ${sdk_info}，无法继续生成 API 清单"; exit 1; }
  IFS="$(printf '\t')" read -r sdk_dir sdk_version < "${sdk_info}"
  [ -d "${sdk_dir}" ] || { log "SDK 目录不存在：${sdk_dir}"; exit 1; }

  log ""
  log "### [3/3] API 清单（${sdk_version}）"
  node "${provider_dir}/scripts/extract-api.mjs" \
    --sdk "${sdk_dir}" \
    --out "${provider_dir}/api" \
    --generated-at "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
else
  log ""
  log "### [2/3] SDK：跳过（--docs-only）"
  log "### [3/3] API 清单：跳过（--docs-only）"
fi

log ""
log "完成：$(date -u +%Y-%m-%dT%H:%M:%SZ)"
if [ "${mode}" != "docs" ] && [ -f "${provider_dir}/api/manifest.json" ]; then
  node --input-type=module -e '
import { readFileSync } from "node:fs";
const m = JSON.parse(readFileSync(process.argv[1], "utf8"));
process.stderr.write(
  `  版本 ${m.version}｜resource ${m.coverage.resources}｜带请求体方法 ${m.coverage.methods}｜请求体类型 ${m.coverage.params_types}\n`,
);
' "${provider_dir}/api/manifest.json"
fi
