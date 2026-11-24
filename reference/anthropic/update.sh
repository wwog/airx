#!/usr/bin/env bash
# 一条命令刷新 Anthropic 参照快照：官方文档 + 官方 npm SDK 裁剪 + API 清单。
#
#   ./update.sh                # docs + sdk + api
#   ./update.sh --docs-only    # 只抓文档（不动 sdk/ 与 api/）
#   ./update.sh --sdk-only     # 只抓 SDK 并重建 api/（不动 docs/）
#
#   SDK_VERSION=1.13.0 ./update.sh          # 固定 SDK 版本
#   FORCE=1 ./update.sh --sdk-only          # 同版本也强制重抓 tarball
#   DOCS_INDEX_URL=https://docs.claude.com/llms.txt ./update.sh --docs-only
#
# 幂等：重复执行只会覆盖同名快照；sdk 版本未变时直接跳过下载。
# 下载缓存落在 raw/、_tmp/（均已 gitignore），不提交。
set -uo pipefail

mode="all"
for arg in "$@"; do
  case "${arg}" in
    --docs-only) mode="docs" ;;
    --sdk-only) mode="sdk" ;;
    -h|--help)
      sed -n '2,16p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *)
      echo "未知参数：${arg}（支持 --docs-only / --sdk-only）" >&2
      exit 2
      ;;
  esac
done

root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck disable=SC1091
. "${root}/versions.env"
mkdir -p "${root}/raw" "${root}/_tmp"

rc=0
t0=$(date +%s)

if [ "${mode}" != "sdk" ]; then
  echo "== [1/3] 抓取官方文档"
  if ! bash "${root}/scripts/fetch-docs.sh"; then
    echo "!! 文档步骤失败（没有任何页面成功），详见 docs/meta.json" >&2
    rc=1
  fi
fi

if [ "${mode}" != "docs" ]; then
  echo "== [2/3] 抓取并裁剪 SDK"
  if ! bash "${root}/scripts/fetch-sdk.sh"; then
    echo "!! SDK 步骤失败" >&2
    exit 1
  fi

  # 与 fetch-sdk.sh 相同的版本解析，用来定位这次要解析的 sdk 目录
  resolved="${SDK_VERSION}"
  if [ "${resolved}" = "latest" ]; then
    resolved="$(npm view "${SDK_PACKAGE}" version 2>/dev/null | tr -d '[:space:]')"
  fi
  sdk_dir="${root}/sdk/$(printf '%s' "${SDK_PACKAGE}" | tr '/' '-')@${resolved}"
  if [ ! -d "${sdk_dir}" ]; then
    echo "!! 未找到 ${sdk_dir#"${root}/"}（sdk 步骤可能刚失败）" >&2
    exit 1
  fi

  echo "== [3/3] 生成 API 清单（静态解析 ${sdk_dir#"${root}/"}）"
  if ! node "${root}/scripts/extract-api.mjs" "${sdk_dir}" "${root}/api"; then
    echo "!! API 清单生成失败" >&2
    exit 1
  fi

  # 自检：coverage 必须等于数组长度，且每个方法都落在某个 resource 下
  node - "${root}/api/manifest.json" <<'NODE'
import { readFileSync } from 'node:fs';
const m = JSON.parse(readFileSync(process.argv[2], 'utf8'));
const problems = [];
if (m.methods.length !== m.coverage.methods) problems.push(`methods: ${m.methods.length} != coverage ${m.coverage.methods}`);
if (m.types.length !== m.coverage.params_types) problems.push(`types: ${m.types.length} != coverage ${m.coverage.params_types}`);
const resources = new Set(m.methods.map((x) => x.resource));
if (resources.size !== m.coverage.resources) problems.push(`resources: ${resources.size} != coverage ${m.coverage.resources}`);
for (const x of m.methods) {
  if (!x.resource || !x.method) problems.push(`bad method entry: ${JSON.stringify(x)}`);
  if (!('http' in x)) problems.push(`missing http key: ${x.resource}.${x.method}`);
}
if (problems.length) {
  console.error('自检失败：\n  ' + problems.slice(0, 20).join('\n  '));
  process.exit(1);
}
console.error(`   自检通过：${m.coverage.resources} resources / ${m.coverage.methods} methods / ${m.coverage.params_types} params types`);
NODE
  if [ $? -ne 0 ]; then
    echo "!! API 清单自检失败" >&2
    exit 1
  fi
fi

echo "== 完成，用时 $(( $(date +%s) - t0 ))s"
[ "${mode}" != "sdk" ] && [ -f "${root}/docs/meta.json" ] && \
  node -e '
    const m = require(process.argv[1] + "/docs/meta.json");
    console.log(`   docs: ${m.totals.pages_ok}/${m.totals.pages_requested} 页, ${m.totals.bytes} 字节` + (m.totals.pages_failed ? `, ${m.totals.pages_failed} 页失败` : ""));
  ' "${root}"
[ "${mode}" != "docs" ] && [ -f "${root}/api/manifest.json" ] && \
  node -e '
    const m = require(process.argv[1] + "/api/manifest.json");
    console.log(`   api:  ${m.package}@${m.version}, ${m.coverage.methods} 方法 / ${m.coverage.params_types} 参数类型`);
  ' "${root}"

exit "${rc}"
