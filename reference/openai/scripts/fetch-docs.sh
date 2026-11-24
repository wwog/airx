#!/usr/bin/env bash
# 抓取 OpenAI 官方 API 文档 markdown 快照。
#
#   用法: scripts/fetch-docs.sh [--best-effort|--strict]
#
# 入口：api/reference/llms.txt（API 参考全量）+ api/docs/llms.txt（指南白名单）。
# 索引里每个条目都有官方 .md 镜像，因此不需要 HTML 转换；只做链接筛选与下载。
# 产物：docs/**/*.md、docs/INDEX.md、docs/meta.json。
#
# 安全：URL 只来自官方索引且必须匹配白名单正则；下载内容只当数据，绝不执行。
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
provider_dir="$(cd "${script_dir}/.." && pwd)"
# shellcheck source=../versions.env
. "${provider_dir}/versions.env"

best_effort="${DOCS_BEST_EFFORT}"
for arg in "$@"; do
  case "${arg}" in
    --best-effort) best_effort=1 ;;
    --strict) best_effort=0 ;;
    -h|--help)
      sed -n '2,10p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *)
      echo "fetch-docs: 未知参数 ${arg}" >&2
      exit 2
      ;;
  esac
done

docs_dir="${provider_dir}/docs"
raw_dir="${provider_dir}/raw/docs"
mkdir -p "${docs_dir}" "${raw_dir}"

generated_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
http_user_agent="airx-reference-snapshot/1.0 (+${SOURCE_REPO})"
TAB="$(printf '\t')"

log() { printf '%s\n' "$*" >&2; }
die() { printf 'fetch-docs: %s\n' "$*" >&2; exit 1; }

# 索引只允许来自官方文档站，且必须是 https
for index_url in "${DOCS_INDEX}" "${DOCS_REFERENCE_INDEX}" "${DOCS_GUIDES_INDEX}"; do
  case "${index_url}" in
    "https://developers.openai.com/"*) ;;
    *) die "索引地址不是官方文档站：${index_url}" ;;
  esac
done

# ---------------------------------------------------------------- 索引下载

fetch_index() {
  local url="$1" dest="$2" attempt=1
  while [ "${attempt}" -le "${DOCS_RETRIES}" ]; do
    if curl -L --fail --silent --show-error \
      --max-time "${DOCS_MAX_TIME}" \
      --user-agent "${http_user_agent}" \
      --output "${dest}.part" --url "${url}"; then
      mv "${dest}.part" "${dest}"
      return 0
    fi
    log "  索引下载失败（第 ${attempt}/${DOCS_RETRIES} 次）：${url}"
    attempt=$((attempt + 1))
    sleep "${attempt}"
  done
  rm -f "${dest}.part"
  return 1
}

log "==> 获取文档索引"
fetch_index "${DOCS_INDEX}" "${raw_dir}/llms.txt" || die "无法获取顶层索引 ${DOCS_INDEX}"
fetch_index "${DOCS_REFERENCE_INDEX}" "${raw_dir}/reference-llms.txt" || die "无法获取参考索引 ${DOCS_REFERENCE_INDEX}"
fetch_index "${DOCS_GUIDES_INDEX}" "${raw_dir}/guides-llms.txt" || die "无法获取指南索引 ${DOCS_GUIDES_INDEX}"

# ---------------------------------------------------------------- 链接筛选

# 从 llms.txt 抽出所有 `[Title](https://developers.openai.com/*.md)`，输出 `url<TAB>title`。
# 用 awk 而不是 sed：BSD sed 的替换段不解析 \t，容易把标题写坏。
extract_links() {
  LC_ALL=C awk '
    {
      rest = $0
      while (match(rest, /\[[^][]*\]\(https:\/\/developers\.openai\.com\/[^()]*\.md\)/)) {
        segment = substr(rest, RSTART, RLENGTH)
        rest = substr(rest, RSTART + RLENGTH)
        split_at = index(segment, "](")
        title = substr(segment, 2, split_at - 2)
        url = substr(segment, split_at + 2)
        sub(/\)$/, "", url)
        gsub(/[[:space:]]+/, " ", title)
        sub(/^ /, "", title)
        sub(/ $/, "", title)
        print url "\t" title
      }
    }
  ' "$1"
}

# 只接受官方域名 + 安全字符 + .md 结尾
valid_url() {
  printf '%s' "$1" | grep -Eq '^https://developers\.openai\.com/[A-Za-z0-9._~/-]+\.md$'
}

# 白名单匹配：slug 形如 `guides/text`（/api/docs/ 之后、去掉 .md）
guide_included() {
  local slug="$1" pattern
  while IFS= read -r pattern; do
    [ -n "${pattern}" ] || continue
    # 不加引号才会按通配符匹配；只可能匹配本地 slug，不涉及文件系统
    # shellcheck disable=SC2254
    case "${slug}" in
      ${pattern}) return 0 ;;
    esac
  done <<EOF
${DOCS_GUIDE_INCLUDE}
EOF
  return 1
}

# 可选排除：体积吃紧时用 DOCS_EXCLUDE 的 URL 通配裁掉个别超大页面
page_excluded() {
  local url="$1" pattern
  while IFS= read -r pattern; do
    [ -n "${pattern}" ] || continue
    # shellcheck disable=SC2254
    case "${url}" in
      ${pattern}) return 0 ;;
    esac
  done <<EOF
${DOCS_EXCLUDE}
EOF
  return 1
}

reject_file="${raw_dir}/rejected.tsv"
pages_tsv="${raw_dir}/pages.tsv"   # kind \t url \t relpath
titles_tsv="${raw_dir}/titles.tsv" # url \t title
: > "${reject_file}"
: > "${pages_tsv}"
: > "${titles_tsv}"

references_links="${raw_dir}/reference-links.tsv"
guides_links="${raw_dir}/guides-links.tsv"
extract_links "${raw_dir}/reference-llms.txt" > "${references_links}"
extract_links "${raw_dir}/guides-llms.txt" > "${guides_links}"

add_page() {
  local kind="$1" url="$2" title="$3" rel
  valid_url "${url}" || { printf 'invalid-url\t%s\n' "${url}" >> "${reject_file}"; return 0; }
  if page_excluded "${url}"; then
    printf 'excluded-by-pattern\t%s\n' "${url}" >> "${reject_file}"
    return 0
  fi
  rel="${url#"${DOCS_SITE}"/}"
  case "${rel}" in
    ""|/*|*..*) printf 'unsafe-path\t%s\n' "${url}" >> "${reject_file}"; return 0 ;;
  esac
  printf '%s\t%s\t%s\n' "${kind}" "${url}" "${rel}" >> "${pages_tsv}"
  printf '%s\t%s\n' "${url}" "${title}" >> "${titles_tsv}"
}

# 参考文档：全量收录
while IFS="${TAB}" read -r url title; do
  [ -n "${url}" ] || continue
  add_page reference "${url}" "${title}"
done < "${references_links}"

# 指南：按白名单过滤
guide_prefix="${DOCS_SITE}/api/docs/"
while IFS="${TAB}" read -r url title; do
  [ -n "${url}" ] || continue
  case "${url}" in
    "${guide_prefix}"*) ;;
    *)
      printf 'non-api-docs\t%s\n' "${url}" >> "${reject_file}"
      continue
      ;;
  esac
  slug="${url#"${guide_prefix}"}"
  slug="${slug%.md}"
  if guide_included "${slug}"; then
    add_page guide "${url}" "${title}"
  else
    printf 'not-in-guide-allowlist\t%s\n' "${url}" >> "${reject_file}"
  fi
done < "${guides_links}"

# 去重并排序（保证输出确定）
LC_ALL=C sort -u -t "${TAB}" -k2,2 "${pages_tsv}" -o "${pages_tsv}"
LC_ALL=C sort -u -t "${TAB}" -k1,1 "${titles_tsv}" -o "${titles_tsv}"

total_pages="$(wc -l < "${pages_tsv}" | tr -d ' ')"
[ "${total_pages}" -gt 0 ] || die "索引里没有解析到任何 .md 链接（站点结构可能变了）"
log "    待抓取页面：${total_pages}"

# ---------------------------------------------------------------- 逐页下载

results_tsv="${raw_dir}/results.tsv"   # status \t bytes \t kind \t url \t relpath
failures_tsv="${raw_dir}/failures.tsv" # url \t relpath
: > "${results_tsv}"
: > "${failures_tsv}"

fetch_page() {
  local url="$1" out="$2" attempt=1
  while [ "${attempt}" -le "${DOCS_RETRIES}" ]; do
    if curl -L --fail --silent --show-error \
      --max-time "${DOCS_MAX_TIME}" \
      --user-agent "${http_user_agent}" \
      --output "${out}" --url "${url}"; then
      [ -s "${out}" ] && return 0
      log "  空响应：${url}"
    fi
    attempt=$((attempt + 1))
    [ "${attempt}" -le "${DOCS_RETRIES}" ] && sleep "${attempt}"
  done
  return 1
}

log "==> 下载文档（间隔 ${DOCS_SLEEP}s，最多 ${DOCS_RETRIES} 次重试）"
index=0
failed=0
updated=0
unchanged=0
while IFS="${TAB}" read -r kind url rel; do
  index=$((index + 1))
  dest="${docs_dir}/${rel}"
  mkdir -p "$(dirname "${dest}")"
  tmp="${dest}.part"
  if fetch_page "${url}" "${tmp}"; then
    if [ -f "${dest}" ] && cmp -s "${tmp}" "${dest}"; then
      status=unchanged
      unchanged=$((unchanged + 1))
      rm -f "${tmp}"
    else
      status=updated
      updated=$((updated + 1))
      mv "${tmp}" "${dest}"
    fi
    bytes="$(wc -c < "${dest}" | tr -d ' ')"
    printf '%s\t%s\t%s\t%s\t%s\n' "${status}" "${bytes}" "${kind}" "${url}" "${rel}" >> "${results_tsv}"
  else
    rm -f "${tmp}"
    failed=$((failed + 1))
    printf '%s\t%s\n' "${url}" "${rel}" >> "${failures_tsv}"
    printf 'failed\t0\t%s\t%s\t%s\n' "${kind}" "${url}" "${rel}" >> "${results_tsv}"
    log "  失败：${url}"
  fi
  if [ $((index % 25)) -eq 0 ]; then
    log "    ${index}/${total_pages}（失败 ${failed}）"
  fi
  sleep "${DOCS_SLEEP}"
done < "${pages_tsv}"

log "    下载完成：${index} 个页面（新增/更新 ${updated}，内容未变 ${unchanged}），失败 ${failed}"

if [ "${failed}" -gt 0 ] && [ "${best_effort}" != "1" ]; then
  log "有 ${failed} 个页面下载失败（清单见 docs/meta.json）。"
  log "加 --best-effort 可保留部分快照并以 0 退出。"
fi

# ---------------------------------------------------------------- 清理旧快照

# 只在「本轮无失败」时删除索引里已消失的旧文件，避免失败时误删
stale_removed=0
stale_list="${raw_dir}/stale.txt"
: > "${stale_list}"
if [ "${failed}" -eq 0 ]; then
  while IFS= read -r existing; do
    [ -n "${existing}" ] || continue
    rel="${existing#"${docs_dir}"/}"
    case "${rel}" in
      INDEX.md|meta.json) continue ;;
    esac
    if ! awk -F"${TAB}" -v target="${rel}" '$5 == target { found = 1 } END { exit found ? 0 : 1 }' "${results_tsv}"; then
      printf '%s\n' "${rel}" >> "${stale_list}"
      rm -f "${existing}"
      stale_removed=$((stale_removed + 1))
    fi
  done <<EOF
$(find "${docs_dir}" -type f -name '*.md' | LC_ALL=C sort)
EOF
fi

# ---------------------------------------------------------------- 生成索引

meta_values="${raw_dir}/meta-values.tsv"
{
  printf '%s\t%s\n' \
    "generated_at" "${generated_at}" \
    "docs_site" "${DOCS_SITE}" \
    "index" "${DOCS_INDEX}" \
    "reference_index" "${DOCS_REFERENCE_INDEX}" \
    "guides_index" "${DOCS_GUIDES_INDEX}" \
    "source_repo" "${SOURCE_REPO}" \
    "stale_removed" "${stale_removed}" \
    "max_bytes" "${DOCS_MAX_BYTES}" \
    "include" "$(printf '%s' "${DOCS_GUIDE_INCLUDE}" | tr '\n' ' ' | tr -s ' ')" \
    "exclude" "$(printf '%s' "${DOCS_EXCLUDE}" | tr '\n' ' ' | tr -s ' ')"
} > "${meta_values}"

node --input-type=module -e "$(cat <<'NODE'
import { readFileSync, writeFileSync } from 'node:fs';

const [resultsPath, titlesPath, rejectsPath, stalePath, valuesPath, outIndex, outMeta] = process.argv.slice(1);
const readLines = (p) => readFileSync(p, 'utf8').split('\n').filter((l) => l.length > 0);
const tab = (line) => line.split('\t');

const values = new Map(readLines(valuesPath).map((l) => {
  const [k, ...rest] = tab(l);
  return [k, rest.join('\t')];
}));
const titles = new Map(readLines(titlesPath).map((l) => {
  const [k, ...rest] = tab(l);
  return [k, rest.join('\t')];
}));

const results = readLines(resultsPath).map((line) => {
  const [status, bytes, kind, url, rel] = tab(line);
  return { status, bytes: Number(bytes), kind, url, rel };
});
const failures = results.filter((r) => r.status === 'failed').map((r) => ({ url: r.url, path: r.rel }));
const ok = results.filter((r) => r.status !== 'failed');
const totalBytes = ok.reduce((sum, r) => sum + r.bytes, 0);
const maxBytes = Number(values.get('max_bytes') ?? 0);
const stale = readLines(stalePath);
const rejectLines = readLines(rejectsPath);
const excluded = {};
for (const line of rejectLines) {
  const [reason, url] = tab(line);
  if (!reason || !url) continue;
  if (!excluded[reason]) excluded[reason] = [];
  excluded[reason].push(url);
}
const excludedCount = Object.values(excluded).reduce((sum, list) => sum + list.length, 0);

const meta = {
  provider: 'openai',
  generated_at: values.get('generated_at'),
  docs_site: values.get('docs_site'),
  source_repo: values.get('source_repo'),
  indexes: {
    llms: values.get('index'),
    reference: values.get('reference_index'),
    guides: values.get('guides_index'),
  },
  policy: {
    reference: 'api/reference/llms.txt 列出的每个 .md 页面全部收录（无二次筛选）',
    guides: 'api/docs/llms.txt 中只收录匹配 DOCS_GUIDE_INCLUDE 的 slug，其余记入 excluded',
    guide_include: (values.get('include') ?? '').split(' ').filter(Boolean),
    exclude_patterns: (values.get('exclude') ?? '').split(' ').filter(Boolean),
    format: '官方 markdown 镜像（页面 URL 追加 .md），无 HTML 转换',
  },
  counts: {
    pages: ok.length,
    reference_pages: ok.filter((r) => r.kind === 'reference').length,
    guide_pages: ok.filter((r) => r.kind === 'guide').length,
    bytes: totalBytes,
    failed: failures.length,
    stale_removed: Number(values.get('stale_removed') ?? 0),
    excluded: excludedCount,
  },
  limits: { max_bytes: maxBytes, over_limit: totalBytes > maxBytes },
  failures,
  stale_removed: stale,
  excluded,
  pages: ok.map((r) => ({ url: r.url, path: r.rel, bytes: r.bytes, kind: r.kind })),
};

writeFileSync(outMeta, `${JSON.stringify(meta, null, 2)}\n`);

const lines = [];
lines.push('# OpenAI API 文档快照索引');
lines.push('');
lines.push(`- 抓取时间（UTC）：\`${meta.generated_at}\``);
lines.push(
  `- 页面数：**${meta.counts.pages}**（reference ${meta.counts.reference_pages} + guide ${meta.counts.guide_pages}），合计 ${(
    totalBytes / 1048576
  ).toFixed(2)} MiB`,
);
lines.push(`- 顶层索引：${meta.indexes.llms}`);
lines.push(`- 参考索引：${meta.indexes.reference}（全量收录）`);
lines.push(`- 指南索引：${meta.indexes.guides}（按 \`versions.env\` 的 \`DOCS_GUIDE_INCLUDE\` 过滤）`);
lines.push(`- 失败：${meta.counts.failed}；本轮因索引变化删除的旧快照：${meta.counts.stale_removed}；主动排除：${excludedCount}`);
lines.push('- 所有页面均为官方 markdown 镜像（页面 URL 追加 `.md`），无 HTML → markdown 转换，内容即官方原文');
lines.push('');
for (const kind of ['reference', 'guide']) {
  const items = ok.filter((r) => r.kind === kind);
  if (items.length === 0) continue;
  lines.push(`## ${kind === 'reference' ? 'API 参考（全量）' : '指南（白名单）'}`);
  lines.push('');
  lines.push('| 页面 | 本地文件 | 大小 |');
  lines.push('| --- | --- | ---: |');
  for (const item of items) {
    const rawTitle = titles.get(item.url) ?? '';
    const title = (rawTitle || item.url).replace(/\|/g, '\\|');
    lines.push(`| [${title}](${item.url}) | [\`${item.path}\`](${item.path}) | ${(item.bytes / 1024).toFixed(1)} KiB |`);
  }
  lines.push('');
}
if (failures.length > 0) {
  lines.push('## 失败页面');
  lines.push('');
  for (const f of failures) lines.push(`- ${f.url}`);
  lines.push('');
}
if (excludedCount > 0) {
  lines.push('## 主动排除（未下载）');
  lines.push('');
  for (const [reason, urls] of Object.entries(excluded)) {
    lines.push(`- **${reason}**：${urls.length} 个`);
  }
  lines.push('');
}
writeFileSync(outIndex, lines.join('\n'));
process.stdout.write(`${meta.counts.pages}\t${totalBytes}\t${failures.length}\t${meta.limits.over_limit ? 1 : 0}\n`);
NODE
)" "${results_tsv}" "${titles_tsv}" "${reject_file}" "${stale_list}" "${meta_values}" \
  "${docs_dir}/INDEX.md" "${docs_dir}/meta.json" > "${raw_dir}/summary.tsv"

IFS="${TAB}" read -r page_count total_bytes failure_count over_limit < "${raw_dir}/summary.tsv"

log "==> 快照：${page_count} 个页面，${total_bytes} 字节（$((total_bytes / 1024)) KiB），失败 ${failure_count}"

if [ "${over_limit}" = "1" ]; then
  log "文档快照超过 DOCS_MAX_BYTES=${DOCS_MAX_BYTES} 上限：请收紧 DOCS_GUIDE_INCLUDE 或缩小参考范围。"
  exit 3
fi

if [ "${failure_count}" -gt 0 ] && [ "${best_effort}" != "1" ]; then
  log "fetch-docs 失败：${failure_count} 个页面未取到（可用 --best-effort 容忍）"
  exit 1
fi

log "fetch-docs 完成"
