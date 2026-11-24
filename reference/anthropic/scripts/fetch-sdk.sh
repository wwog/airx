#!/usr/bin/env bash
# 从 npm 取官方 @anthropic-ai/sdk，裁剪出“分析接口所需”的文件放进 sdk/<pkg>@<version>/。
#
# 安全约定（见 reference/README.md）：
#   - 下载物只当数据：不 require/import 包里的任何 .js，不执行 npm 生命周期脚本。
#     `npm pack` 不跑安装脚本，另外显式加 --ignore-scripts 兜底。
#   - 解包与 tgz 都落在 _tmp/（gitignore），提交进仓库的只有裁剪后的文本文件。
#
# 保留内容：
#   package.json                     版本与入口元数据
#   **/*.d.ts                        全部类型声明（resources / core / lib / internal / 顶层）
#   resources/**/*.js                CJS 实现，用来静态提取 _client.get/post(...) 的 HTTP 动词+路径
#   VERSION / SOURCE                 解析后的版本与来源（含 tarball URL、sha 完整性）
#
# 丢弃：*.d.ts.map、*.mjs / *.mts、*.js.map、CHANGELOG、bin/、_vendor/ 的运行时文件
#       （_vendor 只有 partial-json-parser 的 .js，无 .d.ts）、node_modules（本包不打包依赖）。
# 上游此版本没有 api.md，所以 api/upstream-api.md 不会生成（README 有说明）。
#
# 用法：
#   ./scripts/fetch-sdk.sh              # 按 versions.env（默认 latest）
#   SDK_VERSION=1.13.0 ./scripts/fetch-sdk.sh
#   FORCE=1 ./scripts/fetch-sdk.sh      # 同版本也重新抓取
set -uo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "${here}/.." && pwd)"
# shellcheck disable=SC1091
. "${root}/versions.env"

tmp_dir="${root}/_tmp"
raw_dir="${root}/raw"
mkdir -p "${tmp_dir}" "${raw_dir}"

version="${SDK_VERSION}"
if [ "${version}" = "latest" ]; then
  echo "==> 解析 ${SDK_PACKAGE} 最新版本"
  version="$(npm view "${SDK_PACKAGE}" version 2>"${tmp_dir}/npm-view.err" | tr -d '[:space:]')"
  if [ -z "${version}" ]; then
    echo "无法解析版本号：" >&2
    cat "${tmp_dir}/npm-view.err" >&2
    exit 1
  fi
fi
echo "==> ${SDK_PACKAGE}@${version}"

# 包名 @anthropic-ai/sdk -> 目录名 @anthropic-ai-sdk
dest="${root}/sdk/$(printf '%s' "${SDK_PACKAGE}" | tr '/' '-')@${version}"

if [ "${FORCE:-0}" != "1" ] && [ -f "${dest}/VERSION" ] && \
   [ "$(tr -d '[:space:]' <"${dest}/VERSION")" = "${version}" ]; then
  echo "    已存在 ${dest#"${root}/"}，跳过（FORCE=1 可强制重抓）"
  exit 0
fi

work="$(mktemp -d "${tmp_dir}/sdk.XXXXXX")"
trap 'rm -rf "${work}"' EXIT

echo "==> npm pack"
tarball_name="$(npm pack "${SDK_PACKAGE}@${version}" --pack-destination "${work}" \
  --ignore-scripts --silent 2>"${tmp_dir}/npm-pack.err" | tail -1)"
tarball="${work}/${tarball_name}"
if [ ! -f "${tarball}" ]; then
  echo "npm pack 失败：" >&2
  cat "${tmp_dir}/npm-pack.err" >&2
  exit 1
fi
cp "${tarball}" "${raw_dir}/$(basename "${tarball}")"

echo "==> 解包并裁剪"
mkdir -p "${work}/extract"
tar -xzf "${tarball}" -C "${work}/extract"
pkg="${work}/extract/package"
[ -d "${pkg}" ] || { echo "解包结构异常：缺少 package/ 目录" >&2; exit 1; }

stage="${work}/stage"
mkdir -p "${stage}"

# package.json
cp "${pkg}/package.json" "${stage}/package.json"

# 全部 .d.ts（排除 .d.ts.map；find 用 -type f 精确控制，不跟随符号链接）
( cd "${pkg}" && find . -type f -name '*.d.ts' -print ) | while IFS= read -r rel; do
  mkdir -p "${stage}/$(dirname "${rel}")"
  cp "${pkg}/${rel}" "${stage}/${rel}"
done

# resources/**/*.js（CJS 版）：静态提取 HTTP 动词 + 路径所需
( cd "${pkg}" && find ./resources -type f -name '*.js' -print ) | while IFS= read -r rel; do
  mkdir -p "${stage}/$(dirname "${rel}")"
  cp "${pkg}/${rel}" "${stage}/${rel}"
done

printf '%s\n' "${version}" >"${stage}/VERSION"
{
  echo "package: ${SDK_PACKAGE}"
  echo "version: ${version}"
  echo "registry: ${NPM_REGISTRY_URL}/${SDK_PACKAGE}"
  echo "tarball: ${NPM_REGISTRY_URL}/${SDK_PACKAGE}/-/${tarball_name}"
  echo "fetched_at: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "fetch_tool: npm pack (--ignore-scripts) + tar -xzf"
  echo "trim: **/*.d.ts + resources/**/*.js + package.json (no .map/.mjs/CHANGELOG/bin)"
} >"${stage}/SOURCE"

# 原子替换目标目录
mkdir -p "${root}/sdk"
rm -rf "${dest}.tmp"
cp -R "${stage}" "${dest}.tmp"
rm -rf "${dest}"
mv "${dest}.tmp" "${dest}"

dts=$(find "${dest}" -name '*.d.ts' | wc -l | tr -d ' ')
js=$(find "${dest}/resources" -name '*.js' | wc -l | tr -d ' ')
size=$(find "${dest}" -type f -exec wc -c {} + | awk 'END {printf "%d", $1}')
echo "    ${dest#"${root}/"}: ${dts} 个 .d.ts, ${js} 个 .js, 合计 ${size} 字节"
