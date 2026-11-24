#!/usr/bin/env bash
# 下载官方 openai npm 包并裁剪出「分析接口所需」的最小文件集。
#
#   用法: scripts/fetch-sdk.sh
#
# 流程：npm view 解析版本 -> npm pack 到 raw/sdk/ -> 解包到 _tmp/ -> 按白名单拷贝
#       到 sdk/openai@<version>/（原子替换）-> 写 VERSION / SOURCE。
#
# 安全：不解包执行、不 npm install、不跑 lifecycle script；tarball 只当数据。
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
provider_dir="$(cd "${script_dir}/.." && pwd)"
# shellcheck source=../versions.env
. "${provider_dir}/versions.env"

raw_dir="${provider_dir}/raw/sdk"
tmp_root="${provider_dir}/_tmp"
mkdir -p "${raw_dir}" "${tmp_root}"

log() { printf '%s\n' "$*" >&2; }
die() { printf 'fetch-sdk: %s\n' "$*" >&2; exit 1; }

# 任何 npm 操作都不执行脚本
export npm_config_ignore_scripts=true
export npm_config_audit=false
export npm_config_fund=false
export npm_config_update_notifier=false

# ------------------------------------------------------------- 解析版本

resolve_version() {
  local version="${SDK_VERSION}"
  if [ "${version}" = "latest" ] || [ -z "${version}" ]; then
    version="$(
      npm view "${SDK_PACKAGE}" version --registry "${NPM_REGISTRY}" 2>/dev/null | tr -d '[:space:]'
    )" || die "npm view ${SDK_PACKAGE} version 失败（网络或 registry 问题）"
  fi
  printf '%s' "${version}" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$' ||
    die "版本号不合法：'${version}'（期望 x.y.z 或 x.y.z-tag）"
  printf '%s\n' "${version}"
}

log "==> 解析 ${SDK_PACKAGE} 版本（SDK_VERSION=${SDK_VERSION}）"
version="$(resolve_version)"
log "    解析结果：${SDK_PACKAGE}@${version}"

tarball="$(cd "${raw_dir}" && pwd)/${SDK_PACKAGE}-${version}.tgz"
tarball_url="${NPM_REGISTRY}/${SDK_PACKAGE}/-/${SDK_PACKAGE}-${version}.tgz"

# ------------------------------------------------------------- 下载 tarball

if [ -f "${tarball}" ]; then
  log "    复用已下载的 $(basename "${tarball}")"
else
  log "==> npm pack ${SDK_PACKAGE}@${version}"
  # npm pack 只打包，不安装、不执行上游脚本
  pack_output="$(
    cd "${raw_dir}" && npm pack "${SDK_PACKAGE}@${version}" \
      --registry "${NPM_REGISTRY}" \
      --pack-destination "${raw_dir}" \
      --silent
  )" || die "npm pack ${SDK_PACKAGE}@${version} 失败"
  # npm pack 输出的文件名可能带前导提示行，取最后一行
  packed="$(printf '%s\n' "${pack_output}" | tail -n 1 | tr -d '[:space:]')"
  [ -n "${packed}" ] || die "npm pack 未返回文件名"
  if [ ! -f "${raw_dir}/${packed}" ]; then
    die "npm pack 声称生成 ${packed}，但文件不存在"
  fi
  if [ "${raw_dir}/${packed}" != "${tarball}" ]; then
    mv "${raw_dir}/${packed}" "${tarball}"
  fi
fi

[ -f "${tarball}" ] || die "缺少 tarball：${tarball}"

# 校验归档：拒绝绝对路径与 .. 穿越（tarball 是外部数据）
if tar tzf "${tarball}" | grep -Eq '(^/|(^|/)\.\.(/|$))'; then
  die "tarball 含可疑路径（绝对路径或 ..），已中止"
fi

# ------------------------------------------------------------- 解包 + 裁剪

work="$(mktemp -d "${tmp_root}/sdk.XXXXXX")"
cleanup() { rm -rf "${work}"; }
trap cleanup EXIT

log "==> 解包到临时目录"
tar xzf "${tarball}" -C "${work}" --no-same-owner 2>/dev/null || tar xzf "${tarball}" -C "${work}"
pkg_dir="${work}/package"
[ -f "${pkg_dir}/package.json" ] || die "tarball 结构异常：缺少 package/package.json"

meta_version="$(node --input-type=module -e '
import { readFileSync } from "node:fs";
const pkg = JSON.parse(readFileSync(process.argv[1], "utf8"));
process.stdout.write(String(pkg.version ?? ""));
' "${pkg_dir}/package.json")"
[ "${meta_version}" = "${version}" ] || die "tarball 内版本 ${meta_version} 与期望 ${version} 不一致"

dest="${provider_dir}/sdk/${SDK_PACKAGE}@${version}"
staging="${work}/trimmed"
mkdir -p "${staging}"

log "==> 裁剪（只保留分析所需的类型声明与 http 推导文件）"
# 1) 包元数据
cp "${pkg_dir}/package.json" "${staging}/package.json"

# 2) 顶层 .d.ts（client/index/error 等公共类型入口）；不要 .d.mts 与 .map
find "${pkg_dir}" -maxdepth 1 -type f -name '*.d.ts' -exec cp {} "${staging}/" \;

# 3) resources/**/*.d.ts（resource/method/params 的唯一权威来源）
#    4) resources/**/*.js（从 this._client.<verb>(...) 静态推导 HTTP 动词与路径）
if [ -d "${pkg_dir}/resources" ]; then
  while IFS= read -r file; do
    rel="${file#"${pkg_dir}/"}"
    mkdir -p "${staging}/$(dirname "${rel}")"
    cp "${file}" "${staging}/${rel}"
  done <<EOF
$(find "${pkg_dir}/resources" -type f \( -name '*.d.ts' -o -name '*.js' \) | LC_ALL=C sort)
EOF
fi

# 5) 各 resource 目录下的 api.md（上游文档，最终拼成 api/upstream-api.md）
if [ -d "${pkg_dir}/src/resources" ]; then
  while IFS= read -r file; do
    rel="${file#"${pkg_dir}/"}"
    mkdir -p "${staging}/$(dirname "${rel}")"
    cp "${file}" "${staging}/${rel}"
  done <<EOF
$(find "${pkg_dir}/src/resources" -type f -name 'api.md' | LC_ALL=C sort)
EOF
fi

# VERSION / SOURCE 便于人工核对与 manifest 追溯
printf '%s\n' "${version}" > "${staging}/VERSION"
printf '%s\n' "${tarball_url}" > "${staging}/SOURCE"

# 防御：任何 node_modules / *.tgz / 源实现文件都不应出现在裁剪结果里
if find "${staging}" \( -name 'node_modules' -o -name '*.tgz' -o -name '*.mjs' -o -name '*.d.mts' \) -print -quit | grep -q .; then
  die "裁剪结果里出现了不该保留的文件（node_modules/.tgz/.mjs/.d.mts）"
fi

# ------------------------------------------------------------- 原子替换

mkdir -p "${provider_dir}/sdk"
if [ -d "${dest}" ]; then
  rm -rf "${dest}"
fi
mv "${staging}" "${dest}"

file_count="$(find "${dest}" -type f | wc -l | tr -d ' ')"
total_bytes="$(find "${dest}" -type f -exec wc -c {} + | tail -n 1 | awk '{print $1}')"

log "==> SDK 快照就绪：sdk/${SDK_PACKAGE}@${version}（${file_count} 个文件，${total_bytes} 字节）"
log "    VERSION=$(cat "${dest}/VERSION")"
log "    SOURCE=${tarball_url}"

# 供 update.sh 使用：把解析后的路径与版本写到一行，便于后续步骤引用
printf '%s\t%s\n' "${dest}" "${version}" > "${raw_dir}/latest.tsv"
