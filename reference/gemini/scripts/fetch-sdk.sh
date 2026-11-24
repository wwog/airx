#!/usr/bin/env bash
# 拉取官方 @google/genai npm 包并裁剪为分析所需文件 -> reference/gemini/sdk/@google-genai@<version>/
#
# 原则：只解包、只读、只复制。绝不 import/require 包内代码，也不执行任何安装脚本
# （npm pack 对 registry tarball 不会运行 install 脚本；这里也不使用 npm install）。
set -uo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=../versions.env
. "${here}/versions.env"

sdk_root="${here}/sdk"
raw_dir="${here}/raw"
tmp_dir="${here}/_tmp"
mkdir -p "${sdk_root}" "${raw_dir}" "${tmp_dir}"

say() { printf '%s\n' "$*"; }
die() { say "错误：$*" >&2; exit 1; }

command -v npm >/dev/null 2>&1 || die "未找到 npm"

# ---------------------------------------------------------------------------
# 1. 解析版本
# ---------------------------------------------------------------------------
resolved="${SDK_VERSION}"
case "${resolved}" in
  latest|next|"")
    resolved="$(npm view "${SDK_PACKAGE}" version 2>/dev/null | tr -d '[:space:]')" \
      || die "npm view ${SDK_PACKAGE} version 失败"
    ;;
esac
[ -n "${resolved}" ] || die "无法解析 ${SDK_PACKAGE} 版本"
[ "${resolved}" != "undefined" ] || die "无法解析 ${SDK_PACKAGE} 版本"

pkg_dir_name="$(printf '%s' "${SDK_PACKAGE}" | sed 's#/#-#g')"   # @google/genai -> @google-genai
dest="${sdk_root}/${pkg_dir_name}@${resolved}"
printf '%s\n' "${resolved}" > "${tmp_dir}/sdk-version.txt"

say "==> ${SDK_PACKAGE}@${resolved}"

# ---------------------------------------------------------------------------
# 2. 已存在同版本则跳过（保持幂等；SDK_FORCE=1 强制重做）
# ---------------------------------------------------------------------------
if [ "${SDK_FORCE:-0}" != "1" ] \
   && [ -f "${dest}/VERSION" ] \
   && [ "$(tr -d '[:space:]' < "${dest}/VERSION")" = "${resolved}" ] \
   && [ -s "${dest}/dist/genai.d.ts" ]; then
  say "    已是最新（${dest#${here}/}），跳过下载。SDK_FORCE=1 可强制重做。"
  exit 0
fi

# ---------------------------------------------------------------------------
# 3. npm pack（下载 tarball）
# ---------------------------------------------------------------------------
pack_dir="${tmp_dir}/npm"
rm -rf "${pack_dir}"
mkdir -p "${pack_dir}"
tarball="$(npm pack "${SDK_PACKAGE}@${resolved}" --pack-destination "${pack_dir}" 2>/dev/null | tail -1)"
[ -n "${tarball}" ] && [ -s "${pack_dir}/${tarball}" ] || die "npm pack 未产出 tarball"
say "    tarball: ${tarball} ($(wc -c < "${pack_dir}/${tarball}" | tr -d ' ') 字节)"

# ---------------------------------------------------------------------------
# 4. 解包（纯数据，不执行）
# ---------------------------------------------------------------------------
unpack="${tmp_dir}/unpack"
rm -rf "${unpack}"
mkdir -p "${unpack}"
tar -xzf "${pack_dir}/${tarball}" -C "${unpack}" || die "解包失败"
[ -d "${unpack}/package" ] || die "tarball 结构异常（缺少 package/）"

# ---------------------------------------------------------------------------
# 5. 裁剪复制
#    保留：package.json、README.md、LICENSE、dist/genai.d.ts（单文件含全部类型）、
#          由 dist/index.mjs 静态提取的 HTTP 路径表（纯文本，不保留 1.2MB 实现体）
#    丢弃：dist/*.mjs/.cjs/.map、dist/node|web/**（与 genai.d.ts 近似的重复声明）、
#          dist/vertex_internal（Vertex 平台另一套 API，不在 Gemini Developer API 范围）、
#          dist/tokenizer（本地分词器实现）
# ---------------------------------------------------------------------------
staging="${tmp_dir}/staging"
rm -rf "${staging}"
mkdir -p "${staging}/dist"

src="${unpack}/package"
for f in package.json README.md LICENSE; do
  [ -f "${src}/${f}" ] && cp "${src}/${f}" "${staging}/${f}"
done
[ -f "${src}/dist/genai.d.ts" ] || die "缺少 dist/genai.d.ts（上游布局变化？）"
cp "${src}/dist/genai.d.ts" "${staging}/dist/genai.d.ts"

if [ -f "${src}/dist/index.mjs" ]; then
  # 只保留「方法入口 + 路径模板 + HTTP 动词」三类行，约 12KB，供 extract-api.mjs 推导端点。
  {
    say "# Derived from dist/index.mjs by scripts/fetch-sdk.sh (static grep, code NOT executed)."
    say "# Line format: <original line number in index.mjs>: <source line>"
    awk '
      /^class [A-Za-z_$][A-Za-z0-9_$]*/ { print NR": "$0; next }
      /^let [A-Za-z_$][A-Za-z0-9_$]* = class / { print NR": "$0; next }
      /^function [A-Za-z_$][A-Za-z0-9_$]*\(/ { print NR": "$0; next }
      /^    (async )?[A-Za-z_$][A-Za-z0-9_$]*\(params[,)]/ { print NR": "$0; next }
      /formatMap\(/ { print NR": "$0; next }
      /pathToFunc\("/ { print NR": "$0; next }
      /endpointUrl = / { print NR": "$0; next }
      /httpMethod: / { print NR": "$0; next }
      /[^A-Za-z]method: "/ { print NR": "$0 }
    ' "${src}/dist/index.mjs"
  } > "${staging}/dist/http-routes.txt"
fi

# ---------------------------------------------------------------------------
# 6. 落盘（先写临时目录再替换，避免半成品）
# ---------------------------------------------------------------------------
rm -rf "${dest}"
mkdir -p "${dest}"
cp -R "${staging}/." "${dest}/"

printf '%s\n' "${resolved}" > "${dest}/VERSION"

integrity="$(npm view "${SDK_PACKAGE}@${resolved}" dist.integrity 2>/dev/null | tr -d '[:space:]')"
tarball_url="$(npm view "${SDK_PACKAGE}@${resolved}" dist.tarball 2>/dev/null | tr -d '[:space:]')"
{
  say "package: ${SDK_PACKAGE}"
  say "version: ${resolved}"
  say "registry: ${REGISTRY_URL}"
  say "tarball: ${tarball_url:-https://registry.npmjs.org/${SDK_PACKAGE}/-/${SDK_PACKAGE#@}-${resolved}.tgz}"
  say "integrity: ${integrity:-unknown}"
  say "fetched_at: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  say "kept: package.json, README.md, LICENSE, dist/genai.d.ts, dist/http-routes.txt (derived)"
  say "dropped: dist/index.mjs, dist/index.cjs, dist/index.mjs.map, dist/node/**, dist/web/**, dist/tokenizer/**, dist/vertex_internal/**"
} > "${dest}/SOURCE"

# 缓存 tarball（.gitignore 已忽略 *.tgz）
cp "${pack_dir}/${tarball}" "${raw_dir}/${tarball}" 2>/dev/null || true

say "    -> ${dest#${here}/}"
say "    genai.d.ts: $(wc -c < "${dest}/dist/genai.d.ts" | tr -d ' ') 字节, http-routes.txt: $(wc -c < "${dest}/dist/http-routes.txt" 2>/dev/null | tr -d ' ') 字节"

rm -rf "${pack_dir}" "${unpack}" "${staging}"
exit 0
