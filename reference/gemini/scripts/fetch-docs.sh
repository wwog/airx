#!/usr/bin/env bash
# 抓取 Gemini 官方文档 markdown 快照 -> reference/gemini/docs/
#
# 数据源：https://aistudio.google.com/llms.txt（官方机器可读索引，列出每页的 .md 地址）
# 机制：从索引里挑选作用域内的页面，逐页 curl 下载 markdown 原文，不做 HTML 转换。
# 单个页面失败只记录在 docs/meta.json，不中断整体抓取。
set -uo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=../versions.env
. "${here}/versions.env"

docs_dir="${here}/docs"
raw_dir="${here}/raw"
tmp_dir="${here}/_tmp"
mkdir -p "${docs_dir}" "${raw_dir}" "${tmp_dir}"

now_utc() { date -u +%Y-%m-%dT%H:%M:%SZ; }

# ---------------------------------------------------------------------------
# 抓取作用域（相对 https://aistudio.google.com 的路径 glob）
# 只覆盖 generateContent 主线：核心能力 + 平台资源 API + Live（SDK 也有 live 命名空间）。
# 未纳入：托管 Agent（agents/triggers/environments/credentials/webhooks/voices/decisions）、
# 单个模型介绍页、AI Studio 产品页、法务/计费页。扩展时在下面加一行即可。
# ---------------------------------------------------------------------------
scope_patterns=(
  "docs.md"
  "docs/get-started.md"
  "docs/api-key.md"
  "docs/text-generation.md"
  "docs/structured-output.md"
  "docs/streaming.md"
  "docs/thinking.md"
  "docs/thought-signatures.md"
  "docs/function-calling.md"
  "docs/tools.md"
  "docs/tool-combination.md"
  "docs/code-execution.md"
  "docs/google-search.md"
  "docs/url-context.md"
  "docs/computer-use.md"
  "docs/file-search.md"
  "docs/files.md"
  "docs/file-input-methods.md"
  "docs/document-processing.md"
  "docs/image-understanding.md"
  "docs/audio.md"
  "docs/video-understanding.md"
  "docs/embeddings.md"
  "docs/models.md"
  "docs/long-context.md"
  "docs/long-decode.md"
  "docs/batch-api.md"
  "docs/caching.md"
  "docs/tokens.md"
  "docs/api-errors.md"
  "docs/safety-settings.md"
  "docs/safety-guidance.md"
  "docs/media-resolution.md"
  "docs/prompting-strategies.md"
  "docs/speech-generation.md"
  "docs/image-generation.md"
  "docs/openai.md"
  "docs/migrate.md"
  "docs/migrate-to-interactions.md"
  "docs/deprecations.md"
  "docs/changelog.md"
  "docs/rate-limits.md"
  "docs/troubleshooting.md"
  "docs/libraries.md"
  "docs/interactions-overview.md"
  "docs/live-api/*.md"
  "api-reference.md"
  "api-reference/api-versions.md"
  "api-reference/all-methods.md"
  "api-reference/generate-content.md"
  "api-reference/generate-content/*.md"
  "api-reference/models.md"
  "api-reference/models/*.md"
  "api-reference/files.md"
  "api-reference/files/*.md"
  "api-reference/file-search/*.md"
  "api-reference/caching.md"
  "api-reference/caching/*.md"
  "api-reference/embeddings.md"
  "api-reference/embeddings/*.md"
  "api-reference/batch-api.md"
  "api-reference/batch-api/*.md"
  "api-reference/tokens.md"
  "api-reference/tokens/*.md"
  "api-reference/live.md"
  "api-reference/live/*.md"
)

say() { printf '%s\n' "$*"; }

# ---------------------------------------------------------------------------
# 1. 索引
# ---------------------------------------------------------------------------
fetch_index() {
  local dest="${tmp_dir}/llms.txt"
  if curl -sS -L --fail --max-time "${DOCS_MAX_TIME}" \
      --retry "${DOCS_RETRIES}" --retry-delay 2 \
      -o "${dest}" "${DOCS_INDEX_URL}"; then
    cp "${dest}" "${raw_dir}/llms.txt"
    return 0
  fi
  if [ -s "${raw_dir}/llms.txt" ]; then
    say "警告：索引下载失败，回退到上次缓存 raw/llms.txt" >&2
    cp "${raw_dir}/llms.txt" "${dest}"
    return 0
  fi
  if [ -s "${docs_dir}/llms.txt" ]; then
    say "警告：索引下载失败，回退到上次生成的本地快照 docs/llms.txt" >&2
    cp "${docs_dir}/llms.txt" "${dest}"
    return 0
  fi
  return 1
}

say "==> 索引 ${DOCS_INDEX_URL}"
if ! fetch_index; then
  say "错误：无法获取文档索引，终止 docs 抓取。" >&2
  exit 1
fi

# ---------------------------------------------------------------------------
# 2. 探测记录（写进 meta.json，说明为什么选 llms.txt 作为入口）
# ---------------------------------------------------------------------------
probe_code() { # url -> http code
  curl -sS -o /dev/null --max-time 20 -w '%{http_code}' "$1" 2>/dev/null || printf '000'
}
legacy_llms_code="$(probe_code "${DOCS_INDEX_FALLBACK_URL}")"
legacy_docs_redirect="$(curl -sS -o /dev/null --max-time 20 -w '%{redirect_url}' "${LEGACY_DOCS_URL}" 2>/dev/null)"
[ -n "${legacy_docs_redirect}" ] || legacy_docs_redirect="${LEGACY_DOCS_URL}"

# ---------------------------------------------------------------------------
# 3. 从索引取链接、按作用域过滤
# ---------------------------------------------------------------------------
links_tsv="${tmp_dir}/links.tsv"
# 形如：- [标题](https://aistudio.google.com/docs/xxx.md): 描述
# 用 | 作分隔符：IFS 里的 tab 属于「空白字符」，连续 tab（空字段）会被折叠，导致字段错位
sed -n 's/^- \[\([^]]*\)\](\(https:\/\/aistudio\.google\.com\/[^)]*\)).*/\1|\2/p' \
  "${tmp_dir}/llms.txt" > "${links_tsv}"

in_scope() { # path, query
  local p="$1" q="$2" pat matched=0
  case "${p}" in *.md) ;; *) return 1 ;; esac
  for pat in "${scope_patterns[@]}"; do
    # case 的模式侧不引号，才能按 glob 匹配
    case "${p}" in ${pat}) matched=1; break ;; esac
  done
  [ "${matched}" -eq 1 ] || return 1
  if [ -z "${q}" ]; then
    return 0
  fi
  case "${q}" in
    # generateContent 专用变体：内容是不同的一套（generateContent 而非 Interactions）
    generate-content=true) return 0 ;;
    # v1 变体只保留 generateContent 参考，其余走 v1beta
    api-version=v1)
      case "${p}" in
        api-reference/generate-content.md|api-reference/generate-content/*.md) return 0 ;;
        *) return 1 ;;
      esac ;;
    *) return 1 ;;
  esac
}

selected="${tmp_dir}/selected.tsv"
: > "${selected}"
n_all=0
n_scope=0
while IFS='|' read -r title url; do
  [ -n "${url}" ] || continue
  n_all=$((n_all + 1))
  rest="${url#https://aistudio.google.com/}"
  case "${rest}" in
    *\?*) path="${rest%%\?*}"; query="${rest#*\?}" ;;
    *) path="${rest}"; query="" ;;
  esac
  if in_scope "${path}" "${query}"; then
    n_scope=$((n_scope + 1))
    printf '%s|%s|%s|%s\n' "${path}" "${query}" "${title}" "${url}" >> "${selected}"
  fi
done < "${links_tsv}"

say "    索引链接 ${n_all} 条，作用域内 ${n_scope} 条"

# ---------------------------------------------------------------------------
# 4. 逐页下载
# ---------------------------------------------------------------------------
results="${tmp_dir}/results.tsv"   # status  bytes  http  file  url  title
: > "${results}"
: > "${tmp_dir}/expected-files.txt"

say "==> 下载页面"
while IFS='|' read -r path query title url; do
  fname="$(printf '%s' "${path}" | sed 's#/#__#g')"
  case "${query}" in
    generate-content=true) fname="${fname%.md}--generateContent.md" ;;
    api-version=v1)        fname="${fname%.md}--v1.md" ;;
  esac
  dest="${docs_dir}/${fname}"
  printf '%s\n' "${fname}" >> "${tmp_dir}/expected-files.txt"
  if [ -s "${dest}" ] && [ "${DOCS_REFETCH:-1}" != "1" ]; then
    bytes="$(wc -c < "${dest}" | tr -d ' ')"
    printf 'cached|%s|200|%s|%s|%s\n' "${bytes}" "${fname}" "${url}" "${title}" >> "${results}"
    continue
  fi
  code="$(curl -sS -L --fail --max-time "${DOCS_MAX_TIME}" \
            --retry "${DOCS_RETRIES}" --retry-delay 2 \
            -o "${dest}.part" -w '%{http_code}' "${url}" 2>/dev/null)" || code="${code:-000}"
  if [ -s "${dest}.part" ] && [ "${code}" = "200" ]; then
    bytes="$(wc -c < "${dest}.part" | tr -d ' ')"
    if [ "${bytes}" -ge 200 ]; then
      mv "${dest}.part" "${dest}"
      printf 'ok|%s|%s|%s|%s|%s\n' "${bytes}" "${code}" "${fname}" "${url}" "${title}" >> "${results}"
    else
      rm -f "${dest}.part"
      printf 'failed|0|%s|%s|%s|%s\n' "${code}" "${fname}" "${url}" "${title}" >> "${results}"
      say "    失败 ${fname} (HTTP ${code}, 响应过短)"
    fi
  else
    rm -f "${dest}.part"
    printf 'failed|0|%s|%s|%s|%s\n' "${code:-000}" "${fname}" "${url}" "${title}" >> "${results}"
    say "    失败 ${fname} (HTTP ${code:-000})"
  fi
  sleep "${DOCS_SLEEP}"
done < "${selected}"

# ---------------------------------------------------------------------------
# 5. 清理不在本次清单内的旧快照（INDEX.md / llms.txt 保留）
# ---------------------------------------------------------------------------
removed=0
for f in "${docs_dir}"/*.md; do
  [ -e "${f}" ] || continue
  base="$(basename "${f}")"
  case "${base}" in
    INDEX.md|llms.txt) continue ;;
  esac
  if ! grep -Fqx "${base}" "${tmp_dir}/expected-files.txt"; then
    rm -f "${f}"
    removed=$((removed + 1))
  fi
done
[ "${removed}" -eq 0 ] || say "    清理旧快照 ${removed} 个"

cp "${tmp_dir}/llms.txt" "${docs_dir}/llms.txt"

# ---------------------------------------------------------------------------
# 6. 汇总：docs/meta.json + docs/INDEX.md
# ---------------------------------------------------------------------------
ok=0; failed=0; total_bytes=0
while IFS='|' read -r status bytes code fname url title; do
  case "${status}" in
    ok|cached) ok=$((ok + 1)); total_bytes=$((total_bytes + bytes)) ;;
    *) failed=$((failed + 1)) ;;
  esac
done < "${results}"

json_escape() { sed 's/\\/\\\\/g; s/"/\\"/g' | tr -d '\r'; }

meta="${docs_dir}/meta.json"
{
  printf '{\n'
  printf '  "provider": "gemini",\n'
  printf '  "generated_at": "%s",\n' "$(now_utc)"
  printf '  "site": "%s",\n' "${DOCS_SITE}"
  printf '  "index_url": "%s",\n' "${DOCS_INDEX_URL}"
  printf '  "format": "official markdown (每页 URL 后缀 .md)",\n'
  printf '  "pages_total": %s,\n' "$((ok + failed))"
  printf '  "pages_ok": %s,\n' "${ok}"
  printf '  "pages_failed": %s,\n' "${failed}"
  printf '  "total_bytes": %s,\n' "${total_bytes}"
  printf '  "removed_stale": %s,\n' "${removed}"
  printf '  "probes": {\n'
  printf '    "ai_google_dev_llms_txt_http": "%s",\n' "${legacy_llms_code}"
  printf '    "ai_google_dev_gemini_api_docs_redirect": "%s"\n' "$(printf '%s' "${legacy_docs_redirect}" | json_escape)"
  printf '  },\n'
  printf '  "scope": [\n'
  i=0
  for pat in "${scope_patterns[@]}"; do
    i=$((i + 1))
    if [ "${i}" -lt "${#scope_patterns[@]}" ]; then
      printf '    "%s",\n' "${pat}"
    else
      printf '    "%s"\n' "${pat}"
    fi
  done
  printf '  ],\n'
  printf '  "pages": [\n'
  i=0
  nrows=$(wc -l < "${results}" | tr -d ' ')
  while IFS='|' read -r status bytes code fname url title; do
    i=$((i + 1))
    printf '    {"file": "%s", "url": "%s", "title": "%s", "status": "%s", "http": "%s", "bytes": %s}%s\n' \
      "$(printf '%s' "${fname}" | json_escape)" \
      "$(printf '%s' "${url}" | json_escape)" \
      "$(printf '%s' "${title}" | json_escape)" \
      "${status}" "${code}" "${bytes:-0}" \
      "$([ "${i}" -lt "${nrows}" ] && printf ',' || true)"
  done < "${results}"
  printf '  ]\n'
  printf '}\n'
} > "${meta}"

index_md="${docs_dir}/INDEX.md"
{
  printf '# Gemini 文档快照索引\n\n'
  printf '生成时间（UTC）：%s\n\n' "$(now_utc)"
  printf -- '- 来源索引：%s\n' "${DOCS_INDEX_URL}"
  printf -- '- 站点：%s（`ai.google.dev/gemini-api/docs` 301 到 `aistudio.google.com/docs`）\n' "${DOCS_SITE}"
  printf -- '- 格式：官方 markdown 原文（每页 URL 加 `.md`，未做 HTML 转换）\n'
  printf -- '- 页面：%s 个（成功 %s，失败 %s），合计 %s 字节\n' "$((ok + failed))" "${ok}" "${failed}" "${total_bytes}"
  printf -- '- 机器可读明细：`meta.json`；索引原文：`llms.txt`\n\n'
  printf '> 文件名规则：URL 路径中 `/` 替换为 `__`；`?generate-content=true` 追加 `--generateContent`，`?api-version=v1` 追加 `--v1`。\n\n'
  printf '| 文件 | 标题 | 来源 URL | 字节 | 状态 |\n| --- | --- | --- | ---: | --- |\n'
  while IFS='|' read -r status bytes code fname url title; do
    printf '| `%s` | %s | %s | %s | %s |\n' "${fname}" "${title}" "${url}" "${bytes}" "${status}"
  done < "${results}"
} > "${index_md}"

say "==> docs: ${ok} 成功 / ${failed} 失败，合计 ${total_bytes} 字节"

if [ "${failed}" -gt 0 ]; then
  exit 1
fi
exit 0
