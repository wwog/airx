# reference/openai — OpenAI 官方文档与 SDK 接口快照

本目录是 OpenAI 协议的**只读参照物**：官方 API 文档的 markdown 快照 + 官方 `openai`
npm 包裁剪出的类型声明 + 由这两者静态分析出的全量接口清单。

它不参与 Cargo 构建，也不是运行时依赖。要确认「某个参数/接口官方怎么写」时看这里，
不要凭记忆。

**入库的只有脚本与说明**（`README.md`、`update.sh`、`versions.env`、`scripts/*`）；
`docs/`、`sdk/`、`api/` 全是 `./update.sh` 在本地生成的产物，已由根 `.gitignore`
忽略，**不进版本库**。换一台机器（或刚 clone 完）第一件事就是跑一次刷新：

```bash
./update.sh              # 一条命令全量刷新（docs + sdk + api），生成物留在本地
./update.sh --docs-only  # 只刷新文档
./update.sh --sdk-only   # 只刷新 SDK 与 API 清单
./update.sh --best-effort         # 个别文档页失败也保留部分快照（失败清单进 meta.json）
SDK_VERSION=7.63.0 ./update.sh   # 固定版本
```

> 没有网络 / 不想跑脚本时，`docs/`、`sdk/`、`api/` 目录可能不存在——这是预期状态，
> 不是仓库损坏。判断依据永远以「本机最近一次 `./update.sh`」为准。

## 最近一次运行

| 项 | 值 |
| --- | --- |
| 生成时间（UTC） | `2026-11-24T01:58:39Z`（API 清单）/ `2026-11-24T01:55:07Z`（文档） |
| SDK 版本 | `openai@7.63.0` |
| 文档页面 | 278 页（API 参考 210 + 指南 68），27,775,943 字节 ≈ 26.5 MiB |
| SDK 快照 | 516 个文件，3,964,892 字节 ≈ 3.8 MiB |
| API 清单 | 90 个 resource / 155 个带请求体的方法 / 352 个请求体类型 |
| HTTP 路径可静态推导 | 142 / 155（13 个委托型方法为 `null`，见「已知限制」） |
| 未收录（仅 path/query） | 335 个方法 |

以上数字都来自本机最近一次 `./update.sh` 的产物；换版本后以 `api/manifest.json` 的
`coverage` 和 `docs/meta.json` 的 `counts` 为准。

## 数据来源

| 用途 | URL | 说明 |
| --- | --- | --- |
| 顶层路由索引 | <https://developers.openai.com/llms.txt> | 只用于确认 API 文档入口，不从中取页面 |
| API 文档路由 | <https://developers.openai.com/api/llms.txt> | 指向下面两个子索引 |
| **API 参考（全量）** | <https://developers.openai.com/api/reference/llms.txt> | 列出每个 endpoint 的 `methods/*.md`、各 resource 概览、streaming/websocket 事件手册 |
| **API 指南（白名单）** | <https://developers.openai.com/api/docs/llms.txt> | 概念/指南/模型/库，按 `DOCS_GUIDE_INCLUDE` 过滤 |
| npm 包 | <https://registry.npmjs.org/openai> | `npm pack openai@<version>` |
| 源码仓库 | <https://github.com/openai/openai-node> | 仅作 README/注释引用，不 clone |

**格式**：OpenAI 对每个文档页都提供 markdown 镜像（页面 URL 追加 `.md`），
索引里给的就是 `.md` 链接。因此本快照是**官方原文**，没有 HTML → markdown 转换，
也就不存在转换保真度损失。

## 收录范围（抓什么、不抓什么）

抓取集合 = `api/reference/llms.txt` 全部 210 个 `.md` 页面
+ `api/docs/llms.txt` 里命中白名单的 68 个 `.md` 页面。

- **参考文档：全量**。包含 chat/completions/responses/embeddings/files/batches/
  audio/images/moderations/fine-tuning/vector_stores/uploads/containers/evals/
  conversations/realtime/beta/*、admin API，以及 `streaming-events`、
  `websocket-events` 事件手册。协议实现要逐字段对照的就是这批。
- **指南：白名单**（`versions.env` 的 `DOCS_GUIDE_INCLUDE`，按 `/api/docs/` 之后的
  slug 做通配匹配），只保留接口行为相关页：`concepts`、`quickstart`、`changelog`、
  `deprecations`、`libraries*`、`models*`，以及 `guides/` 下的
  text、completions、streaming-responses、structured-outputs、function-calling、
  tools*、reasoning、prompt-caching、embeddings、batch、file-inputs、
  image-generation、images-vision、moderation、rate-limits、token-counting、
  conversation-state、background、webhooks、error-codes、compaction、
  flex-processing、predicted-outputs、migrate-to-responses、evals、graders、
  audio*、speech-to-text、text-to-speech、transcription、realtime*、voice-webrtc、
  voice-websockets、websocket-mode、fine-tuning*。
- **明确排除**（本轮 181 个页面）：Agents API / Agent Builder / ChatKit / Codex /
  GPT Actions / Plugins / MCP 服务端、Terraform、企业安全合规（RBAC、mTLS、
  private link、IP allowlist、workload identity federation）、定价、
  supported countries、教程。理由是它们描述产品配置或其它运行时，不是本仓库要
  对齐的 HTTP API 行为。被排除的 URL 全量记录在 `docs/meta.json` 的
  `excluded` 字段里（按原因分组），可见、可复查。

想改变范围：编辑 `versions.env` 的 `DOCS_GUIDE_INCLUDE`（新增）或 `DOCS_EXCLUDE`
（按 URL 通配裁掉个别超大页面，例如 `*/beta/subresources/responses/*`），
然后 `./update.sh --docs-only`。`api/reference/llms.txt` 的新端点会自动全量跟上。

## 抓取机制

`scripts/fetch-docs.sh`：

1. 下载三个索引到 `raw/docs/`（失败即退出，索引是后续步骤的前提）。
2. 用 awk 从 llms.txt 抽 `[Title](url)`，URL 必须匹配
   `^https://developers\.openai\.com/[A-Za-z0-9._~/-]+\.md$`；路径含 `..` 或绝对路径的直接拒绝。
3. 参考索引全量收录；指南索引按白名单过滤；其余写进 `raw/docs/rejected.tsv`。
4. 逐页 `curl -L --fail --silent --show-error --max-time 60`，最多重试 3 次，
   页间 `sleep 0.2`（礼貌抓取）。先写 `.part`，与旧文件 `cmp` 相同则保留旧文件
   （内容未变不重写 mtime），不同才 `mv` 覆盖。
5. 单页失败时默认整体以 1 退出，并已写出 `docs/meta.json`（含失败 URL 列表）；
   `--best-effort` 可保留部分快照并以 0 退出。
6. 本轮无失败时，删除索引里已消失的旧页面（记入 `meta.json.stale_removed`）。
7. 由 Node 生成 `docs/INDEX.md` 与 `docs/meta.json`（JSON 转义交给
   `JSON.stringify`，不手拼字符串）。

本地路径 = URL 去掉 `https://developers.openai.com/`，保留目录层级：例如
`api/reference/resources/chat/subresources/completions/methods/create.md` →
`docs/api/reference/resources/chat/subresources/completions/methods/create.md`。

重跑时不用 ETag、每次都重新 GET，但只有内容变化才落盘（其余只删 `.part`），
所以重跑不会制造无意义的 mtime 变动，生成物内容也逐字节稳定。

`scripts/fetch-sdk.sh`：

1. `SDK_VERSION=latest` 时用 `npm view openai version` 解析，否则用给定版本（校验
   `x.y.z[-tag]` 形式）。
2. `npm pack openai@<version> --pack-destination raw/sdk`（设置
   `npm_config_ignore_scripts=true`，绝不 install、绝不执行上游脚本）。已有
   tarball 时复用。
3. 解包前检查归档内没有绝对路径或 `..` 穿越；解包到 `_tmp/sdk.XXXXXX`。
4. 校验 `package/package.json` 的 version 与期望一致。
5. 裁剪（白名单拷贝）：
   - `package.json`
   - 顶层 `*.d.ts`（`client/index/error/pagination/streaming/...`）
   - `resources/**/*.d.ts`（248 个，resource / method / params 的唯一权威来源）
   - `resources/**/*.js`（248 个，只用于静态推导 HTTP 动词 + 路径）
   - `src/resources/*/api.md`（5 个，最终拼成 `api/upstream-api.md`）
   - 自写 `VERSION`、`SOURCE`
6. 原子替换 `sdk/openai@<version>/`；防御性检查确认结果里没有
   `node_modules`、`*.tgz`、`*.mjs`、`*.d.mts`。

**没有保留**：`lib/`、`internal/`、`core/`、`src/`（除 api.md）、`azure.*`/`bedrock.*`
运行时实现、全部 `*.map`、`.d.mts` 变体、`CHANGELOG.md`、`README.md`。
整包解包 34 MB / 4107 文件，裁剪后 3.8 MiB / 516 文件。

## API 清单生成

`scripts/extract-api.mjs`（Node，无 TypeScript 依赖，不 import 上游代码）：

- **resource / method**：扫 `resources/**/*.d.ts` 中
  `export declare class X extends APIResource`，取 4 空格缩进的方法声明；支持跨行
  签名与嵌套泛型（如 `parse<Params extends ..., ParsedT = F<Params>>(body: Params)`）。
  resource 名由路径推导：`resources/chat/completions/completions.d.ts` → `chat.completions`。
- **请求体**：只有带 `body` 实参的方法算「带请求体」。该 SDK 里 `params` 有时是 body
  （`beta.threads.runs.create` 先 `const { include, ...body } = params`），有时只是
  query/path 参数（`roles.delete`），所以用同名 `.js` 里 options 对象的
  `body` 键反推真实 body 实参名，而不是靠参数名猜。
- **HTTP**：在同名 `.js` 里找 `this._client.<verb>(... )` 的首个字面量，模板串
  `${responseID}` 归一为 `{responseID}`；`getAPIList` → GET；options 里显式
  `method: 'post'` 覆盖 helper 名（例如批量 activate）。
- **类型**：`*Params` 接口（含 `ParamsBase` / `ParamsNonStreaming` / `ParamsStreaming`）
  按「name + 声明文件」逐条列出顶层字段；同名类型在不同 resource 模块里各自声明
  （47 个名字有重复声明，例如 `FileCreateParams` 在 containers/files 和
  vector_stores/files 里字段不同），不合并。`export type XParams = A | B` 这类别名
  无 interface 体，保留名字并标 `unresolved`，其成员接口单独成条。
- 输出前对 methods / types 排序，`generated_at` 可由 `--generated-at` 固定，
  因此同一 SDK 版本重复运行结果逐字节一致。

产物大小：`manifest.json` 345 KB、`SUMMARY.md` 78 KB、`upstream-api.md` 50 KB。

## 产物

| 路径 | 内容 | 入库 |
| --- | --- | --- |
| `README.md` / `update.sh` / `versions.env` / `scripts/*` | 说明与脚本 | ✅ |
| `docs/api/reference/**/*.md` | API 参考全量快照（210 页） | ❌（本地生成） |
| `docs/api/docs/**/*.md` | 白名单指南（68 页） | ❌（本地生成） |
| `docs/INDEX.md` | 中文索引：每个页面的官方 URL、本地路径、体积 | ❌（本地生成） |
| `docs/meta.json` | 抓取时间、页面清单（url/path/bytes/kind）、失败列表、排除清单与原因、体积上限 | ❌（本地生成） |
| `sdk/openai@7.63.0/**` | 裁剪后的 `.d.ts` + `resources/**/*.js` + `package.json` + `VERSION`/`SOURCE` | ❌（本地生成） |
| `api/manifest.json` | 全量机器可读清单（methods / types / coverage） | ❌（本地生成） |
| `api/SUMMARY.md` | 人读版：按 resource 分组的方法表 + 请求体字段表 | ❌（本地生成） |
| `api/upstream-api.md` | 上游自带的 `src/resources/*/api.md` 拼接（本版本无顶层 `api.md`） | ❌（本地生成） |
| `raw/`、`_tmp/`、`*.tgz` | 下载缓存与解包临时目录 | ❌ |

根 `.gitignore` 已忽略 `/reference/**/{docs,sdk,api}/` 与 `raw/`、`_tmp/`、`*.tgz`，
所以 `git status --short -uall reference/openai` 只会看到脚本与说明这几个文件；
`docs/`、`sdk/`、`api/` 删了也能用 `./update.sh` 复原。

`manifest.json` 的 `coverage` 与数组长度一致：`resources` = methods 里去重后的
resource 数，`methods` = `methods[]` 长度，`params_types` = `types[]` 长度
（interface 328 + 类型别名 24 = 352）。

## 刷新后怎么用

因为生成物不入库，**不能用 `git diff` 跟踪接口变化**。要看有没有变，先自己留一份旧产物：

```bash
cd reference/openai
cp api/SUMMARY.md /tmp/SUMMARY.before.md
cp docs/meta.json /tmp/meta.before.json
./update.sh
diff /tmp/SUMMARY.before.md api/SUMMARY.md     # 接口/参数变化
diff /tmp/meta.before.json docs/meta.json      # 文档页增减、失败
```

只想确认「现在这份快照是什么版本」：看 `api/manifest.json` 的 `version` /
`generated_at` 与 `docs/meta.json` 的 `counts`。

- 想知道某方法的 HTTP 路径与参数：`api/SUMMARY.md` 对应 resource 小节。
- 想知道某字段的类型/是否可选/嵌套结构：`api/manifest.json` 的
  `types[].field_details`，或直接读 `sdk/.../*.d.ts`。
- 想读官方原文：`docs/api/reference/resources/<resource>/methods/<method>.md`。

自检（数字应与 `coverage` 完全对应）：

```bash
node --input-type=module -e '
import { readFileSync } from "node:fs";
const m = JSON.parse(readFileSync("api/manifest.json", "utf8"));
const d = JSON.parse(readFileSync("docs/meta.json", "utf8"));
console.log(m.coverage,
  m.coverage.resources === new Set(m.methods.map((x) => x.resource)).size,
  m.coverage.methods === m.methods.length,
  m.coverage.params_types === m.types.length,
  d.counts);
'
```

## 已知限制

1. **13 个方法的 HTTP 路径为 `null`**：这些方法把请求委托给 `lib/*.js` 辅助函数或
   其它 resource（`embeddings.create`、`beta.threads.*` 的 poll/stream 变体、
   `vector-stores.*.createAndPoll`、`beta.agents.environments.files.upload`）。
   对应的 `lib/` 未纳入裁剪，因此静态不可见。需要时以官方参考文档中的
   `**post** \`/path\`` 为准。
2. **`ChatCompletionParseParams` 这类 helper 参数类型只保留名字**：它声明在
   `lib/ChatCompletionStream.d.ts`（未纳入裁剪），故 `params_resolved: false`。
   同类还有 4 个 `export type` 别名（`ExternalKeyCreateParams` 等），字段由其成员
   接口承载。
3. **335 个方法不在清单里**：只有 path/query 参数（list/retrieve/delete/cancel/
   count 等），不产生请求体。它们的签名仍在 `sdk/**/*.d.ts` 里，可随时查。
4. **路径参数名保持 SDK 原样**：`{containerID}` 与 `{container_id}` 这类命名不一致
   都照抄 SDK，未做归一化，方便直接对照。
5. **本地体积**：文档 26.5 MiB（不入库，只占本机磁盘），其中 4 个事件/响应体手册
   就占一半：`reference/resources/responses.md` 5.7 MB、
   `beta/subresources/responses/websocket-events.md` 3.1 MB、
   `beta/subresources/responses/streaming-events.md` 2.6 MB、
   `responses/websocket-events.md` 2.6 MB。未超过 `DOCS_MAX_BYTES`（40 MiB）；
   若磁盘吃紧，用 `DOCS_EXCLUDE` 裁掉，`meta.json` 会记录被排除的 URL。
6. **解析是静态文本分析，不是 TypeScript 编译器**：极端写法（正则字面量里含引号、
   模板串里嵌套反引号）可能干扰括号匹配；当前 `openai@7.63.0` 实测覆盖 134 个
   class 文件、全部 129 个显式 `body` 方法，无遗漏。
7. 快照是**时间点**数据；OpenAI 文档与 SDK 都在快速变化，结论以你手上的
   `VERSION` / `meta.json` 为准。
