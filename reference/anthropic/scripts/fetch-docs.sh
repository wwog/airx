#!/usr/bin/env bash
# 抓取 Anthropic 官方 Claude API 文档的 markdown 快照。
#
# 数据源：https://platform.claude.com/llms.txt（官方文档索引）
#   - 索引里每一项都是 markdown URL（形如 .../docs/en/api/messages/create.md），
#     直接 curl 就能拿到带 YAML frontmatter 的纯 markdown，不需要 HTML 抓取。
#   - 不加 .md 后缀会返回 1.2MB 的 Next.js 空壳 HTML（正文靠 JS 渲染），
#     所以本脚本只抓 .md，并在写盘后校验首行，防止把 HTML 存成 markdown。
#
# 输出：
#   docs/llms.txt        索引原文件
#   docs/<path>.md       逐页 markdown，路径 = 索引 URL 去掉 /docs/en/ 前缀
#   docs/INDEX.md        本地路径 <-> 源 URL 对照表
#   docs/meta.json       抓取时间、逐页状态、总字节数、收录/排除范围
#
# 单页失败不会中断：失败项写入 meta.json 的 entries[].status = "failed"。
#
# 用法：./scripts/fetch-docs.sh
set -uo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "${here}/.." && pwd)"
# shellcheck disable=SC1091
. "${root}/versions.env"

docs_dir="${root}/docs"
tmp_dir="${root}/_tmp"
mkdir -p "${docs_dir}" "${tmp_dir}"

tsv="${tmp_dir}/docs-meta.tsv"
: >"${tsv}"
generated_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

# 收录清单：分组|URL 相对路径（相对 ${DOCS_BASE_URL}，不含 .md）
# 选页原则：协议实现/参数审查会用到的 API 参考 + 核心行为指南。
# 未收录的大片区域（admin/organization、managed-agents、agents-and-tools 的
# 具体工具页、模型卡、多语言、release-notes 等）见 README 与 meta.json。
DOC_PAGES=$(cat <<'EOF'
API 概览|api/overview
API 概览|api/versioning
API 概览|api/errors
API 概览|api/rate-limits
API 概览|api/beta-headers
API 概览|api/service-tiers
Messages|api/messages
Messages|api/messages/create
Messages|api/messages/count_tokens
Messages|api/messages/batches
Messages|api/messages/batches/create
Messages|api/messages/batches/list
Messages|api/messages/batches/retrieve
Messages|api/messages/batches/results
Messages|api/messages/batches/cancel
Messages|api/messages/batches/delete
Models|api/models
Models|api/models/list
Models|api/models/retrieve
Files|api/files
Files|api/files/upload
Files|api/files/list
Files|api/files/retrieve_metadata
Files|api/files/download
Files|api/files/delete
指南|build-with-claude/overview
指南|build-with-claude/streaming
指南|build-with-claude/structured-outputs
指南|build-with-claude/prompt-caching
指南|build-with-claude/cache-diagnostics
指南|build-with-claude/thinking
指南|build-with-claude/extended-thinking
指南|build-with-claude/thinking-tool-workflows
指南|build-with-claude/thinking-steering-and-cost
指南|build-with-claude/thinking-troubleshooting
指南|build-with-claude/preserved-thinking
指南|build-with-claude/token-counting
指南|build-with-claude/batch-processing
指南|build-with-claude/files
指南|build-with-claude/working-with-messages
指南|build-with-claude/handling-stop-reasons
Tool use|agents-and-tools/tool-use/overview
Tool use|agents-and-tools/tool-use/define-tools
Tool use|agents-and-tools/tool-use/how-tool-use-works
Tool use|agents-and-tools/tool-use/handle-tool-calls
Tool use|agents-and-tools/tool-use/tool-reference
Tool use|agents-and-tools/tool-use/parallel-tool-use
Tool use|agents-and-tools/tool-use/strict-tool-use
Tool use|agents-and-tools/tool-use/fine-grained-tool-streaming
Tool use|agents-and-tools/tool-use/tool-use-with-prompt-caching
模型与价格|models/overview
模型与价格|about-claude/pricing
EOF
)

# --- 取索引 ---------------------------------------------------------------
echo "==> 索引 ${DOCS_INDEX_URL}"
index_http="$(curl -sS -L --fail --max-time "${CURL_MAX_TIME}" \
  --retry "${CURL_RETRIES}" --retry-delay "${CURL_RETRY_DELAY}" --retry-all-errors \
  -o "${docs_dir}/llms.txt" -w '%{http_code}' "${DOCS_INDEX_URL}" 2>"${tmp_dir}/docs-index.err")"
index_rc=$?
if ((index_rc != 0)) || [ "${index_http}" != "200" ]; then
  echo "    索引抓取失败 (rc=${index_rc}, http=${index_http})，见 ${tmp_dir}/docs-index.err" >&2
  index_http="failed"
fi

# --- 逐页抓取 -------------------------------------------------------------
# 注意：循环体在管道子 shell 里，计数器不能跨迭代累加；所有汇总都由后面的
# node 步骤从 TSV 重新计算，这里只负责逐页落盘 + 记录状态。
printf '%s\n' "${DOC_PAGES}" | while IFS='|' read -r group rel; do
  [ -z "${rel:-}" ] && continue
  url="${DOCS_BASE_URL}/${rel}.md"
  out="${docs_dir}/${rel}.md"
  mkdir -p "$(dirname "${out}")"
  printf '    %-58s ' "${rel}"

  http="$(curl -sS -L --fail --max-time "${CURL_MAX_TIME}" \
    --retry "${CURL_RETRIES}" --retry-delay "${CURL_RETRY_DELAY}" --retry-all-errors \
    -o "${out}.part" -w '%{http_code}' "${url}" 2>"${tmp_dir}/doc.err")"
  rc=$?

  status="ok"
  err=""
  size=0
  title=""
  if ((rc != 0)) || [ "${http}" != "200" ]; then
    status="failed"
    err="curl rc=${rc} http=${http}: $(tr '\n' ' ' <"${tmp_dir}/doc.err" | cut -c1-200)"
    rm -f "${out}.part"
  elif head -c 64 "${out}.part" | grep -qi '<!DOCTYPE'; then
    # 防御：拿到 HTML 空壳说明源站行为变了，不要写进快照
    status="failed"
    err="response is HTML, not markdown"
    rm -f "${out}.part"
  else
    mv "${out}.part" "${out}"
    size="$(wc -c <"${out}" | tr -d ' ')"
    # frontmatter 的 title:（有的页面没有，退回文件里第一个 # 标题）
    title="$(sed -n '1,12p' "${out}" | sed -n 's/^title:[[:space:]]*//p' | head -1)"
    if [ -z "${title}" ]; then
      title="$(sed -n 's/^#[[:space:]]*//p' "${out}" | head -1)"
    fi
  fi
  rm -f "${out}.part"
  title="$(printf '%s' "${title}" | tr '\t' ' ' | tr -d '\r')"

  printf '%s %8s\n' "${status}" "${size}"
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
    "${group}" "${rel}" "${url}" "${status}" "${size}" "${title}" "${err}" >>"${tsv}"
  sleep "${FETCH_SLEEP}"
done

# --- 汇总 + 生成 INDEX.md / meta.json -------------------------------------
# 用 node 读 TSV 生成 JSON，避免在 shell 里手工拼 JSON 触发转义问题。
node - "${tsv}" "${docs_dir}" "${generated_at}" "${DOCS_INDEX_URL}" \
  "${DOCS_INDEX_ALT_URL}" "${DOCS_BASE_URL}" "${index_http}" "${root}" <<'NODE'
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const [tsv, docsDir, generatedAt, indexUrl, indexAlt, baseUrl, indexHttp, root] =
  process.argv.slice(2);

const rows = readFileSync(tsv, 'utf8')
  .split('\n')
  .filter(Boolean)
  .map((line) => {
    const [group, rel, url, status, bytes, title, error] = line.split('\t');
    return {
      group,
      rel,
      url,
      status,
      bytes: Number(bytes) || 0,
      title: title || '',
      error: error || '',
    };
  });

const ok = rows.filter((r) => r.status === 'ok');
const failed = rows.filter((r) => r.status !== 'ok');
const totalBytes = ok.reduce((sum, r) => sum + r.bytes, 0);

let indexBytes = 0;
let indexEntries = 0;
try {
  const raw = readFileSync(join(docsDir, 'llms.txt'), 'utf8');
  indexBytes = Buffer.byteLength(raw);
  indexEntries = (raw.match(/^- \[/gm) || []).length;
} catch {
  indexBytes = 0;
}

const meta = {
  provider: 'anthropic',
  generated_at: generatedAt,
  index: {
    url: indexUrl,
    alt_url: indexAlt,
    alt_url_note: 'docs.claude.com/llms.txt 与 platform.claude.com/llms.txt 返回逐字节相同内容，规范主机取 platform.claude.com',
    http_status: indexHttp === '200' ? 200 : indexHttp,
    bytes: indexBytes,
    markdown_entries_in_index: indexEntries,
  },
  base_url: baseUrl,
  format: 'official markdown (.md)，非 HTML 抓取',
  scope: {
    included: [
      'API 参考：Messages / count_tokens / Batches / Models / Files',
      'API 通用：overview / versioning / errors / rate-limits / beta-headers / service-tiers',
      '核心指南：streaming、structured outputs、prompt caching、thinking（含 extended-thinking/preserved）、token counting、batch processing、files、working-with-messages、stop reasons',
      'Tool use：overview / define-tools / how-tool-use-works / handle-tool-calls / tool-reference / parallel / strict / fine-grained streaming / caching',
      'Models overview 与 pricing',
    ],
    excluded: [
      'api/beta/**（beta 命名空间下的全部端点页；类型层面由 sdk/ 的 .d.ts 覆盖）',
      'organization/** 与 admin（API keys、RBAC、workspaces、spend limits 等组织管理端点）',
      'managed-agents/**、agents-and-tools 下的具体工具页（bash/computer-use/text-editor/web-search 等）',
      'cli-sdks-libraries/**（各语言 SDK 文档）、release-notes/**、models/<model>/**（逐模型卡与 system prompts）',
      '非英文语言版本（de/es/fr/it/ja/ko/pt-BR/ru/zh-CN/zh-TW/id），llms.txt 只内联英文全文',
    ],
  },
  totals: {
    pages_requested: rows.length,
    pages_ok: ok.length,
    pages_failed: failed.length,
    bytes: totalBytes,
    bytes_with_index: totalBytes + indexBytes,
  },
  entries: rows.map(({ rel, url, status, bytes, title, error, group }) => ({
    path: `${rel}.md`,
    group,
    url,
    status,
    bytes,
    title,
    ...(error ? { error } : {}),
  })),
};

writeFileSync(join(docsDir, 'meta.json'), JSON.stringify(meta, null, 2) + '\n');

// INDEX.md
const groups = [];
for (const row of rows) {
  let g = groups.find((x) => x.name === row.group);
  if (!g) groups.push((g = { name: row.group, rows: [] }));
  g.rows.push(row);
}
const esc = (s) => s.replace(/\|/g, '\\|');
const lines = [
  '# Anthropic 文档快照索引',
  '',
  `- 生成时间（UTC）：${generatedAt}`,
  `- 索引源：${indexUrl}`,
  `- 页面基址：${baseUrl}`,
  `- 请求 ${rows.length} 页，成功 ${ok.length} 页，失败 ${failed.length} 页，正文合计 ${totalBytes} 字节（另有 llms.txt ${indexBytes} 字节）`,
  '- 全部页面来自官方 markdown 端点（URL 以 `.md` 结尾），未做 HTML 抓取/转换。',
  '',
  '`本地路径` 相对本目录；`源 URL` 为 `docs/en/<路径>.md`。',
];
for (const g of groups) {
  lines.push('', `## ${g.name}`, '', '| 本地路径 | 标题 | 大小 | 源 URL |', '| --- | --- | --- | --- |');
  for (const r of g.rows) {
    lines.push(
      `| \`${r.rel}.md\` | ${esc(r.title) || '—'} | ${r.bytes} | ${r.url} |`,
    );
  }
}
if (failed.length) {
  lines.push('', '## 抓取失败', '', '| 本地路径 | 源 URL | 错误 |', '| --- | --- | --- |');
  for (const r of failed) {
    lines.push(`| \`${r.rel}.md\` | ${r.url} | ${esc(r.error)} |`);
  }
}
lines.push('');
writeFileSync(join(docsDir, 'INDEX.md'), lines.join('\n'));

console.error(
  `==> docs: ${ok.length}/${rows.length} 页, ${totalBytes} 字节 (llms.txt ${indexBytes})` +
    (failed.length ? `, ${failed.length} 页失败` : ''),
);
NODE

# 兜底统计（node 已打印，这里给调用方一个退出码约定）：
# 一页都没成功 => 失败；部分失败 => 0（明细在 meta.json），避免阻断 SDK 步骤。
node -e '
const m = JSON.parse(require("node:fs").readFileSync(process.argv[1] + "/docs/meta.json", "utf8"));
process.exit(m.totals.pages_ok === 0 ? 1 : 0);
' "${root}"
