# reference/gemini — Gemini 官方文档 + `@google/genai` API 快照

本目录是 Gemini 的**离线行为参照**：官方文档 markdown 快照、官方 npm SDK 的裁剪副本、
以及由 SDK 类型声明静态生成的 API 清单。它不参与 Cargo 构建，也不是运行时依赖。
与 airx 实现的逐项差距见 [`GAP.md`](GAP.md)。
约定见上级 [`../README.md`](../README.md)。

**入库范围**：仓库里**只提交脚本与本 README**（`README.md`、`update.sh`、`versions.env`、`scripts/*`）。
`docs/`、`sdk/`、`api/` 是**本地生成物，已被根 `.gitignore` 忽略，不入库**。
新机器上先跑一次 `./update.sh` 即可重新生成全部快照（约 2–4 分钟，取决于网络）。

原则：**只读取、不执行**下载内容。脚本只解析 `.md` / `.d.ts` / `package.json` / bundle 文本，
绝不 `import` / `require` / `eval` 上游代码，也不运行其安装脚本。

```bash
./update.sh                 # docs + sdk + api 全量刷新（一条命令）
./update.sh --docs-only     # 只抓文档
./update.sh --sdk-only      # 只拉 SDK 并重建 API 清单
SDK_VERSION=3.6.1 ./update.sh          # 固定版本
SDK_FORCE=1 ./update.sh                # 版本未变也重新下载 SDK
DOCS_SLEEP=0.5 ./update.sh --docs-only # 放慢抓取节奏
```

---

## 1. 数据源调查结论（决定抓取机制的依据）

| 探测 | 结果 | 结论 |
| --- | --- | --- |
| `https://ai.google.dev/gemini-api/docs` | **301** → `https://aistudio.google.com/docs`（200，`content-type: text/markdown`） | 旧域名只是跳转，真正的文档站是 AI Studio |
| `https://aistudio.google.com/llms.txt` | **200**，约 72 KB，官方机器可读索引（章节 + 每页链接） | **选为唯一入口** |
| `https://ai.google.dev/llms.txt` | **404** | 旧域名没有索引，不可用 |
| 每页加 `.md` 后缀（如 `/docs/text-generation.md`） | 200，`text/markdown` | **官方 markdown 管线，无需 HTML 转换** |
| `?generate-content=true` 变体 | 200，内容为 generateContent 版本 | 同一路径存在两套内容，需按 query 区分保存 |
| `?api-version=v1` 变体 | 200，v1 参考 | 仅对 generateContent 参考保留 |
| `https://aistudio.google.com/llms-full.txt` | 200，约 10.5 MB（全站拼接） | 体积大且不可按页 diff，**不使用**，仅记录 |
| `https://ai.google.dev/sitemap.xml` | 200 → `sitemap_0_of_1.xml`（约 320 KB，117 条 `<loc>`，混大量 `?hl=` hreflang 变体，`lastmod` 多为 2025–2026 上半年） | devsite 旧站点残留，**不用作索引** |
| `https://ai.google.dev/robots.txt` | `Disallow: /*?*authuser=`，指向上面的 sitemap | 抓 `.md` 页面不受限 |

**结论：文档站有稳定的官方 markdown 管线**（`llms.txt` 索引 + 每页 `.md`）。因此本快照是
**原文 markdown**，不是 HTML 抓取转换 —— 上游改版时不需要维护选择器/转换器。
唯一需要跟随的变化是：`llms.txt` 的链接结构，或页面路径改名（脚本按索引取链接，路径变了会自动跟随；
若某页被删，则该页记为 failed 并从 `docs/` 清理）。

注意：官方当前把 **Interactions API** 宣称为默认（`generateContent` 被视为 legacy）。
本快照按任务范围仍以 **generateContent 主线**为主：凡同时存在两套内容的能力页，
优先保存 `?generate-content=true` 版本（28 页）。

## 2. 抓取机制（`scripts/fetch-docs.sh`）

1. 下载 `https://aistudio.google.com/llms.txt` 到 `raw/llms.txt`（失败时回退上次缓存或上次生成的本地快照）。
2. 用 `sed` 抽出 `- [标题](URL)` 链接，按 `scope_patterns`（脚本内数组）筛选作用域。
3. 逐页 `curl -sS -L --fail --max-time 60 --retry 3 --retry-delay 2`，页间 `DOCS_SLEEP`（默认 0.15s）。
4. 单页失败（非 200 / 响应 < 200 字节）不中断：删除半成品，记入 `docs/meta.json` 的 `pages[]`（`status: failed` + HTTP 码）。
5. 结尾清理本次清单之外的旧 `.md`（保留 `INDEX.md`、`llms.txt`），并重写 `docs/INDEX.md`、`docs/meta.json`。
6. 有任何页面失败时脚本以退出码 1 结束（`update.sh` 汇总报告，但仍完成后续步骤）。

**文件名规则**（确定性，便于 diff）：URL 路径 `/` → `__`；`?generate-content=true` 追加 `--generateContent`；
`?api-version=v1` 追加 `--v1`。例：`api-reference/generate-content/generate-content.md`
→ `docs/api-reference__generate-content__generate-content.md`。

**作用域**（相对 `aistudio.google.com`，见脚本内 `scope_patterns`）：
generateContent / streamGenerateContent 参考、structured output、streaming、thinking、thought signatures、
function calling 与 tools、code execution、google search、url context、file search、files、
embeddings、models、batch、caching、tokens、errors、safety、media resolution、live-api、
以及 files/models/caching/embeddings/batch/tokens/file-search 的平台 API 参考。
**未纳入**：托管 Agent（agents/triggers/environments/credentials/webhooks/voices/decisions）、
单个模型介绍页、AI Studio 产品页、计费/法务页。需要时在 `scope_patterns` 加一行即可。

## 3. SDK 裁剪（`scripts/fetch-sdk.sh`）

- 包：**`@google/genai`**（当前统一 GenAI SDK）。
  **不要**用已弃用的 `@google/generative-ai`（最后版本 0.24.1）或 `google-generativeai`；
  官方 `llms.txt` 也明确要求使用 `google-genai` / `@google/genai`。
- 流程：`npm view` 解析版本 → `npm pack` 拉 tarball（存 `raw/`，gitignore）→ `tar xzf` 到 `_tmp/` →
  只复制分析所需文件 → 先写暂存目录再替换，避免半成品 → 清理临时目录。
- tarball 布局（3.6.1）：`package/{package.json,README.md,LICENSE}`、`dist/genai.d.ts`（828 KB，**单文件含全部类型**）、
  `dist/index.mjs|.cjs`（1.27 MB 实现）、`dist/index.mjs.map`、`dist/{node,web}/`（与 `genai.d.ts` 近乎重复的声明）、
  `dist/tokenizer/`、`dist/vertex_internal/`。

| 保留 | 说明 |
| --- | --- |
| `package.json`、`README.md`、`LICENSE` | 元数据与上游用法说明 |
| `dist/genai.d.ts` | 全部类型 + 资源类 + JSDoc，API 清单的唯一解析输入 |
| `dist/http-routes.txt` | **派生文件**（约 61 KB）：从 `dist/index.mjs` 静态 grep 出「类/函数边界 + `formatMap`/`pathToFunc` 路径模板 + HTTP 动词」行，用于恢复 REST 端点 |

**丢弃**：`dist/index.mjs|.cjs|.map`（实现体，1.2 MB+）、`dist/node/**`、`dist/web/**`（重复声明）、
`dist/tokenizer/**`（本地分词器）、`dist/vertex_internal/**`（Vertex/企业平台另一套 API）。
不 vendor `node_modules`，不保留整包。

版本未变且 `VERSION` + `dist/genai.d.ts` 完好时跳过下载（`sdk/` 目录内容不变）；`SDK_FORCE=1` 强制重做。
`VERSION`、`SOURCE` 记录版本、tarball URL、`dist.integrity`、抓取时间与保留/丢弃清单。

## 4. API 清单（`scripts/extract-api.mjs`）

纯静态解析，无 TypeScript 依赖、无网络：

1. 把 `genai.d.ts` 的注释/字符串替换为空格（保留偏移），按 `declare class|interface|type|enum|namespace` 建立顶层声明索引与花括号配对。
2. 从 `GoogleGenAI` 类的 `readonly x: X` 字段和 `get x(): X` 取值器发现**资源**；再展开子资源
   （`fileSearchStores.documents`、`live.music`、`interactions.authTokens`、`environments.files` 等），
   并把 `chats.create()` 返回的 `Chat`、`live.connect()` 返回的 `Session` 作为方法集合收录。
3. 解析类体 depth-1 成员：`name: (params) => Ret`、`name(args): Ret`、`get name(): Ret`；
   参数类型优先取名 `params`/`parameters`/`body`/`request` 的那个，其次取唯一的对象类型参数（跳过 `string`/`boolean` 等原始类型）。
4. 重载去重：同一 `resource+method` 保留「params 可解析」优先、其次 params、再次 returns 最完整的签名。
5. **HTTP 端点**：从 `dist/http-routes.txt` 恢复。bundle 里有两套实现风格：
   BaseModule 类内部方法用 `formatMap('{model}:generateContent')` + `httpMethod: 'POST'`；
   next-gen 请求构造器用 `pathToFunc("/{api_version}/agents")` + `method: "POST"`（包装函数名形如 `agentsCreate`）。
   同类名在 bundle 中可能重复（`Files$1` 与 `Files`），用「该 d.ts 类有多少方法能在此 bundle 类里找到路径」打分消歧；
   同一方法多候选端点（如 `embedContent` 的单条/批量）输出主路径 + `http_alternatives`。
   路径规范化：`{api_version}`→`v1beta`、`{model}`→`models/{model}`、`{models_url}`→`models`、`upload/...` 保留前缀。
   Vertex 专属模板（`batchPredictionJobs`、`tuningJobs`、`:predict`、`:fetchPredictOperation`）在有 Gemini API 模板时被排除。
   类型声明里看不到的少数端点在 `manualHttp` 显式登记（目前只有 `files.upload`、`operations.get`），
   `http_source` 字段区分 `sdk-bundle` / `manual`。
6. **类型清单（与另外两家的口径对齐）**：静态收录全部 `interface` / `class` / `enum` / `type` 声明
   （跳过生成器内部的 `X$` 原名，改用 `Agents.Agent` 这类公开别名收录；`fields` 为 depth-1 字段，
   格式 `字段名?: 类型`），然后按名字分成两个数组：
   - `types[]`：**只放请求参数类型**（`*Parameters` / `*Params`），共 154 条。
     共享契约 `coverage.params_types === types.length` 成立（openai 352/352、anthropic 466/466 同口径）。
   - `other_types[]`：其余 1324 条（响应体、枚举、工具类型、错误类型等），`coverage.other_types === other_types.length`。
   - `coverage.types_total = params_types + other_types = 1478`。
   联合类型（`A | B | {...}`）与 `Omit<...> & ...` 组合不做字段合并，保留 `alias` 原文并标 `unresolved: true`
   （约定要求：不能省略，必须标注）。空接口（如 `ActivityEnd{}`）不算 unresolved。
   合计 379 条 unresolved（`types[]` 63 + `other_types[]` 316），多为组合/联合类型。

输出：`api/manifest.json`（机读全量）、`api/SUMMARY.md`（中文分组表 + 总数）、
`api/upstream-api.md`（上游未提供 `api.md`，此处写指针说明并列出替代入口）。

`manifest.json` 顶层键：`provider`、`package`、`version`、`generated_at`（UTC ISO-8601）、
`source`、`http_source`、`types_note`、
`coverage{resources, methods, params_types, other_types, types_total}`、`methods[]`、`types[]`（仅请求参数类型）、
`other_types[]`（其余类型）。脚本内置一致性校验：`coverage.params_types === types.length`、
`coverage.other_types === other_types.length`、`types_total === params_types + other_types`、
`resources === 去重资源数`、`methods === methods.length`，且不得出现重复 `resource.method`，
任一不满足即失败退出。

## 5. 输出与实际体积（最近一次运行）

运行时间：docs 抓取 `2026-11-24T01:56:21Z`；SDK `fetched_at: 2026-11-24T01:32:56Z`；
`api/manifest.json` 生成 `2026-11-24T02:01:17Z`。

| 产物 | 实际值 |
| --- | --- |
| SDK 版本 | **`@google/genai@3.6.1`**（`dist-tags.latest`） |
| 文档页数 / 总字节 | **138 页 / 5,599,991 字节**（约 5.3 MiB；`docs/` 含 `INDEX.md`、`llms.txt` 共 5.7 MB） |
| 其中 generateContent 变体页 / v1 变体页 | 28 / 3 |
| 文档失败页 | 0（探测记录：`ai.google.dev/llms.txt` = 404，`ai.google.dev/gemini-api/docs` → `aistudio.google.com/docs`） |
| 资源（resource） | **24** |
| 方法（method） | **104**，其中 **87** 有 REST 端点 |
| `coverage` | `resources: 24`、`methods: 104`、`params_types: 154`、`other_types: 1324`、`types_total: 1478` |
| `types[]`（请求参数类型） / `other_types[]` / 合计 | **154 / 1324 / 1478**（`coverage.params_types === types.length`） |
| unresolved（`types[]` 63 + `other_types[]` 316） | **379** |
| `api/manifest.json` | 476 KB |
| `sdk/@google-genai@3.6.1/` | 928 KB（`genai.d.ts` 828 KB + `http-routes.txt` 61 KB + 元数据） |

24 个资源（含子资源与返回值对象）：
`models`、`files`、`batches`、`caches`、`tunings`、`operations`、`authTokens`、
`fileSearchStores`、`fileSearchStores.documents`、`chats`、`chat`、
`live`、`live.music`、`live.session`、`interactions`、`interactions.authTokens`、
`agents`、`triggers`、`environments`、`environments.files`、`credentials`、`voices`、`decisions`、`webhooks`。

几条关键端点（详见 `api/SUMMARY.md`）：

| 方法 | HTTP |
| --- | --- |
| `models.generateContent` | `POST /v1beta/models/{model}:generateContent` |
| `models.generateContentStream` | `POST /v1beta/models/{model}:streamGenerateContent?alt=sse` |
| `models.embedContent` | `POST /v1beta/models/{model}:embedContent`（alt: `:batchEmbedContents`） |
| `models.countTokens` | `POST /v1beta/models/{model}:countTokens` |
| `models.list` / `models.get` | `GET /v1beta/models` / `GET /v1beta/{name}` |
| `files.upload` / `files.list` | `POST /upload/v1beta/files` / `GET /v1beta/files` |
| `caches.create` / `caches.list` | `POST /v1beta/cachedContents` / `GET /v1beta/cachedContents` |
| `batches.create` / `createEmbeddings` | `POST /v1beta/models/{model}:batchGenerateContent` / `:asyncBatchEmbedContent` |

**没有 HTTP 的 17 个方法**（属实现方式，不是遗漏）：`chats.create`、`chat.sendMessage`、`chat.sendMessageStream`、
`chat.getHistory`（本地会话包装，内部走 `models.generateContent`）；`live.connect`、`live.music.connect`、
`interactions.connect` 与 5 个 `live.session.*`（WebSocket/WebRTC）；`files.download`、`files._registerFiles`、
`fileSearchStores.downloadMedia`、`environments.files.download`、`environments.files.upload`（走 uploader/downloader 实现，无静态路径常量）。

## 6. 目录结构

```
reference/gemini/
├── README.md                       # 本文件                                    ✅ 入库
├── update.sh                       # ./update.sh [--docs-only|--sdk-only]       ✅ 入库
├── versions.env                    # 包名/版本/索引 URL/抓取参数                ✅ 入库
├── scripts/
│   ├── fetch-docs.sh               # llms.txt -> docs/*.md + INDEX.md + meta.json ✅ 入库
│   ├── fetch-sdk.sh                # npm pack -> 裁剪 -> sdk/... + VERSION/SOURCE ✅ 入库
│   └── extract-api.mjs             # sdk -> api/manifest.json + SUMMARY.md       ✅ 入库
├── docs/                           # 官方 markdown 快照                        ❌ 本地生成，不入库
├── sdk/@google-genai@<version>/    # 裁剪后的类型声明与元数据                   ❌ 本地生成，不入库
├── api/                            # manifest.json / SUMMARY.md / upstream-api  ❌ 本地生成，不入库
├── raw/                            # 索引与 tarball 缓存                        ❌ 本地缓存，不入库
└── _tmp/                           # 解包与中间文件                             ❌ 本地缓存，不入库
```

根 `.gitignore` 用 `/reference/**/{docs,sdk,api,raw,_tmp}/` 与 `/reference/**/*.tgz` 覆盖以上生成/缓存目录，
所以 `git status` 里不会出现这些文件。

## 7. 刷新流程

```bash
cd reference/gemini
./update.sh                      # 生成 docs/ + sdk/ + api/（首次/新机器同样一条命令）
```

- 生成的 `docs/`、`sdk/`、`api/` 都在 `.gitignore` 里：**不入库**，也不需要用 git 管理。
  仓库里只有脚本与 `versions.env`；新克隆的机器跑一次 `./update.sh` 即可得到同一份快照。
- 想看这次更新带来了什么变化（因为没有 git 基线），用文件系统对比：
  ```bash
  cp -R api /tmp/gemini-api-before && ./update.sh --sdk-only
  diff -u /tmp/gemini-api-before/SUMMARY.md api/SUMMARY.md   # 接口/参数变化
  node -e 'console.log(require("./api/manifest.json").coverage)'  # 计数变化
  ```
  文档同理：对 `docs/` 做同样的一次快照拷贝再 `diff -ru`，或用 `docs/meta.json` 的 `pages[]`（含字节数、HTTP 状态）比对。
- 幂等：同名快照覆盖；同版本 SDK 跳过下载；`extract-api.mjs` 连跑两次除 `generated_at` 外输出逐字节一致
  （文档内容随上游更新而变属预期）。
- 想固定某版 SDK：`SDK_VERSION=3.5.0 ./update.sh`；随后 `sdk/` 下会同时存在多个版本目录，
  `update.sh` 按 `_tmp/sdk-version.txt`（本次解析出的版本）生成 `api/`，旧版本目录可手动删除。
- 入库内容仅：`README.md`、`update.sh`、`versions.env`、`scripts/*`。

## 8. 已知限制与判断

1. **文档是官方 markdown，但含模板指令**：页面内有 `{% mark %}…{% /mark %}`、`{% table %}`、`{% details %}`
   这类 Google Docs 渲染标签，以及 `> ## AI Agent Context` 前置块。它们是原文的一部分，未做清洗
   （清洗会破坏与官方 diff 的对应关系）；阅读时跳过 `{% %}` 即可，表格/折叠块里的正文是完整保留的。
2. **Interactions API 已取代 generateContent 成为官方默认**：`llms.txt` 明确写着 generateContent 是 legacy。
   本快照为对齐现有协议实现仍以 generateContent 为主，因此 `docs/` 中同一能力可能出现两份（基础版 + `--generateContent` 版）。
3. **作用域是裁剪过的**（138 页 / 5.3 MiB，远低于 40 MB 上限）：托管 Agent、单模型介绍页、AI Studio 产品页、
   计费法务页未抓。这些 endpoint 在 `api/manifest.json` 里有（SDK 覆盖），只是没有对应文档页。
4. **HTTP 端点来自 bundle 静态 grep**：依赖 `formatMap(...)`/`pathToFunc(...)` 与 `httpMethod:`/`method:` 的书写形式。
   若上游重构请求层（例如把模板拼进变量、改名为 `endpointUrl2`、或全量换生成器），端点列会退化为 `—`，
   **不会给出错误路径**；此时看 `api/manifest.json` 的 `http_source` 字段并按需补 `manualHttp`。
5. **`{name}` 是完整资源名**：SDK 对 `models.get`/`caches.get` 等使用 `formatMap('{name}')`，实际值形如
   `models/<id>`、`cachedContents/<id>`，所以清单里写作 `GET /v1beta/{name}`（附 `http_note`），
   与文档中的 `GET /v1beta/models/{model}` 等价。
6. **379 个 unresolved 类型**（`types[]` 63 + `other_types[]` 316）：多为 `Omit<...> & X["body"]`、
   跨命名空间联合、`(string & {})` 这类组合，静态解析无法得到稳定的顶层字段。它们保留了名称与 `alias` 原文，
   未省略。`Agents.*` / `Interactions.*` 这类公开别名以 `命名空间.类型名` 收录（生成器内部的 `X$` 原名不单独列出）。
7. **`files.upload` / `operations.get` 端点是人工登记的**（`http_source: "manual"`）：
   前者路径是传给 `ApiClient.fetchUploadUrl` 的字符串常量，后者是通用 LRO getter。
8. **`api/upstream-api.md` 不是上游文档副本**：`@google/genai@3.6.1` 没有自带 `api.md`（`docs/` 目录也没有），
   该文件是脚本生成的指针说明。上游用法说明保留在 `sdk/.../README.md`。
9. **文档正文未做语言筛选**：站点返回英文原文，`?hl=` 本地化变体未抓取。
