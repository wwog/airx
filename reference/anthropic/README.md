# anthropic — Claude 官方文档 + SDK API 清单快照

本目录是 **Anthropic Claude API 的离线行为参照**：官方文档 markdown 快照、官方
`@anthropic-ai/sdk` 裁剪后的类型声明、以及从 SDK 静态生成的完整 API 清单。
不参与 Cargo 构建，不是运行时依赖；只用于“官方到底怎么写的”这类问题。

约定见上一级 [`../README.md`](../README.md)。

**入库的只有脚本与说明**（`README.md`、`update.sh`、`versions.env`、`scripts/*`）；
`docs/`、`sdk/`、`api/` 是本地生成物，已被根 `.gitignore` 忽略，换机器时跑一次
`./update.sh` 即可重建。

## 一条命令刷新

```bash
cd reference/anthropic
./update.sh                 # 文档 + SDK + API 清单（默认取 npm latest）
./update.sh --docs-only     # 只重抓文档
./update.sh --sdk-only      # 只重抓 SDK 并重建 api/（不动 docs/）

SDK_VERSION=1.13.0 ./update.sh       # 固定 SDK 版本
FORCE=1 ./update.sh --sdk-only       # 同版本也强制重下 tarball
DOCS_INDEX_URL=https://docs.claude.com/llms.txt ./update.sh --docs-only
```

幂等：重复执行只会覆盖同名快照；`sdk/` 里已存在同版本目录时直接跳过下载。
文档单页失败不会中断流程，失败项写进 `docs/meta.json` 的 `entries[].status`。
`update.sh` 最后会自检 `coverage.*` 是否等于数组长度，不等则以非零码退出。

刷新后看变化（生成物不入库，跨版本对比前先自行备份）：

```bash
cp api/SUMMARY.md /tmp/anthropic-summary.prev.md   # 刷新前
./update.sh
diff /tmp/anthropic-summary.prev.md api/SUMMARY.md # 接口/参数有没有变
```

仓库只提交 `README.md`、`update.sh`、`versions.env`、`scripts/`；`docs/`、`sdk/`、
`api/` 均为本地生成物，已被根目录 `.gitignore` 忽略，换机器时跑一次 `update.sh` 重建。

## 目录与体积

| 路径 | 内容 | 大小（最近一次） |
| --- | --- | --- |
| `README.md` | 本文件 | 15 KB |
| `update.sh` | 编排 docs → sdk → api，支持 `--docs-only` / `--sdk-only` | 4.2 KB |
| `versions.env` | 包名、版本、文档索引 URL、curl 重试参数（环境变量可覆盖） | 1.4 KB |
| `scripts/fetch-docs.sh` | 抓 `llms.txt` + 逐页 markdown → `docs/`、`docs/INDEX.md`、`docs/meta.json` | 11 KB |
| `scripts/fetch-sdk.sh` | `npm pack` + 解包裁剪 → `sdk/@anthropic-ai-sdk@<version>/` | 4.5 KB |
| `scripts/extract-api.mjs` | 静态解析 SDK → `api/manifest.json`、`api/SUMMARY.md` | 27 KB |
| `docs/` | 52 个文档页 + `llms.txt` + `INDEX.md` + `meta.json` | 3.6 MB |
| `sdk/@anthropic-ai-sdk@1.13.0/` | 583 个文件：363 个 `.d.ts` + 217 个 `resources/**/*.js` + `package.json`/`VERSION`/`SOURCE` | 4.0 MB（拼接后字节数） |
| `api/` | `manifest.json`（201 KB）、`SUMMARY.md`（57 KB）；上游无 `api.md`，故无 `upstream-api.md` | 265 KB |
| `raw/`、`_tmp/`、`*.tgz` | 下载缓存、解包临时目录 | ❌ gitignore，不提交 |

单页最大的是 `docs/api/messages.md`（1.3 MB，Messages 全量参考），其余多在 10–160 KB。

## 数据来源与抓取机制

### 1. 文档：官方 markdown，不做 HTML 抓取

探测结果（2026-11-24）：

| 入口 | HTTP | 大小 | 结论 |
| --- | --- | --- | --- |
| `https://platform.claude.com/llms.txt` | 200 | 108,297 B | **规范主机**（llms.txt 自己的 Root URL 与 Console 都指向它） |
| `https://docs.claude.com/llms.txt` | 200 | 108,297 B | 与上者**逐字节相同**（md5 一致），保留为备用 |

`llms.txt` 要点：

- 索引里 **每一项都是 markdown URL**：`https://platform.claude.com/docs/en/<path>.md`，
  英文全文共 **958** 条，其中 `/api/` 下 **614** 条（含 beta 命名空间）。
- 直接 `curl` 这些 `.md` 地址返回纯 markdown：YAML frontmatter（`title`、`url`）+
  正文，**请求行也在正文里**（例如 `**POST** \`/v1/messages\``），header/body/response
  参数以缩进列表逐字段列出。这正是做参数对照需要的形态，无需任何转换。
- **HTML 降级不可用**：去掉 `.md` 后缀返回的是 1.2 MB 的 Next.js 应用空壳
  （`text/html`，正文由 JS 渲染，curl 拿到的是空 div）。因此本快照 **只走 markdown 端点**，
  不实现 HTML 抓取——真需要 HTML 时的保真度会低到没有参考价值，这一点明确记在
  `docs/meta.json` 的 `format` 字段里。
- 抓取行为：`curl -L --fail --max-time 60`，`--retry 3 --retry-delay 2 --retry-all-errors`，
  每页之间 `sleep 0.2`；写入前先落 `.part` 再 `mv`，并校验首行不是 `<!DOCTYPE`（防止源站
  行为变化把 HTML 存成 `.md`）。文件名由 URL 路径确定性推导：`docs/en/api/messages/create.md`
  → `docs/api/messages/create.md`。
- 多语言：`llms.txt` 只内联英文全文，`/docs/zh-CN` 等 357 页需另访网站，本快照**不收录**。

### 2. SDK：npm tarball，只读不执行

- `npm view @anthropic-ai/sdk version` → 当前解析到 **1.13.0**；tarball
  `https://registry.npmjs.org/@anthropic-ai/sdk/-/anthropic-ai-sdk-1.13.0.tgz`，
  解包后 18.3 MB / 约 2000 个文件（含 `.map`、`.mjs`、`bin/`、CHANGELOG 等）。
- 包布局：`client.d.ts`（`Anthropic`/`BaseAnthropic` 类，声明 `messages/models/files/
  skills/userProfiles/organization/beta` 七个根 resource）、`resources/**`（每个 resource
  一对 `x.d.ts` + `x.js` + `.d.ts.map` + `.mjs`）、`core/`、`lib/`、`internal/`、
  `helpers/`、`_vendor/`。
- **此版本没有 `api.md`**（只有 `README.md` / `CHANGELOG.md`），所以 `api/upstream-api.md`
  不会生成；`scripts/fetch-sdk.sh` 会在存在时自动复制。
- **此版本没有 `completions` resource**：`client.d.ts` 与 `resources/index.d.ts` 里都搜不到
  `completions`/`Completions`，即 legacy text completions 已从 v1 线移除。清单里出现的是
  `messages`（含 `messages.batches`）、`models`、`files`、`skills`、`userProfiles`、
  `organization.*`、`beta.*`。
- 安全：`npm pack --ignore-scripts`，解包只 `tar -xzf`；`extract-api.mjs` 用
  `readFileSync` 读文本，**从不 `require`/`import` 包内代码**，也不运行安装脚本。

裁剪规则（`scripts/fetch-sdk.sh`）：

| 保留 | 原因 |
| --- | --- |
| 全部 `**/*.d.ts`（363 个） | class / interface / type alias 的唯一权威来源 |
| `resources/**/*.js`（217 个，仅 CJS） | 静态提取 `this._client.get/post/...('<path>')` 的 HTTP 动词与路径 |
| `package.json`、`VERSION`、`SOURCE` | 版本与来源溯源 |

丢弃：`*.d.ts.map`、`*.js.map`、`*.mts`、`*.mjs`、`bin/`、`CHANGELOG.md`、`node_modules`。
没有引入整包，也没有引入 `_vendor/partial-json-parser` 的运行时 `.js`（它没有 `.d.ts`，
与接口分析无关）。

### 3. API 清单：纯静态解析

`scripts/extract-api.mjs`（Node 24，零依赖、不联网、不用 TypeScript 编译器）：

1. 自实现注释剥离 + 括号配对 + 顶层成员切分（跟踪 `()`/`[]`/`{}` **和泛型 `<>`**，
   并把 `=>` 当箭头而非泛型闭合，否则 `PagePromise<A, B>` 会在逗号处被切断）。
2. 从 `.d.ts` 取 `export declare class X extends APIResource` 的方法签名、`export interface
   XParams` 的顶层字段、`export type XParams = A | B` 的别名。
3. resource 层级用**运行时属性名**（不是猜目录名）：从 `client.d.ts` 里 `messages: API.Messages;`
   这类声明出发，经 `resources/index.d.ts` 的导出表解析到文件，再用每个 `.js` 里
   `this.batches = new BatchesAPI.Batches(this._client)`（含 `require` 别名解析）递归下钻，
   得到 `beta.organization.rbacGroups.members` 这样的真实路径。
4. HTTP 端点三级降级：① `_client.<verb>('<字面量路径>')` / `getAPIList('<path>')`；
   ② 路径是运行时变量时（如 `messages.batches.results` 用 `batch.results_url`），取同方法内
   tracing 元数据里的 `route: '/v1/messages/batches/{message_batch_id}/results'`；
   ③ 都没有 → `null`（不猜）。路径里的 `${x}` 归一成 `{x}`。

## 生成物

以下 `docs/`、`sdk/`、`api/` 都是本地生成物，**不入库**，由 `update.sh` 重建。

### `docs/`

- `llms.txt`：索引原文（108 KB），便于下次 diff 出“官方新加了哪些页”。
- `<path>.md`：52 页正文，路径镜像官方 URL。
- `INDEX.md`：分组表格（本地路径 / 标题 / 字节数 / 源 URL），末尾附失败页（本次为空）。
- `meta.json`：抓取时间、索引状态、`scope.included`/`scope.excluded`、总量、逐页
  `{path, group, url, status, bytes, title}`；失败页额外带 `error`。

`docs/meta.json` 的时间统一 UTC ISO-8601（`date -u +%Y-%m-%dT%H:%M:%SZ`），macOS/Linux 一致。

### `api/manifest.json`

```json
{
  "provider": "anthropic",
  "package": "@anthropic-ai/sdk",
  "version": "1.13.0",
  "generated_at": "2026-11-24T01:34:06Z",
  "source": "https://registry.npmjs.org/@anthropic-ai/sdk",
  "coverage": { "resources": 117, "methods": 399, "params_types": 466 },
  "methods": [{ "resource": "messages", "method": "create", "params": "MessageCreateParams",
                "returns": "APIPromise<Stream<RawMessageStreamEvent> | Message>",
                "http": "POST /v1/messages" }],
  "types":   [{ "name": "MessageCreateParams", "file": "resources/messages/messages.d.ts",
                "fields": ["max_tokens", "messages", "model", "..."] }]
}
```

- `methods[]` 覆盖全部 resource 的全部公开方法（含没有参数的，`params: null`，
  例如 `retrieve(id)` / `delete(id)` / `toolRunner`）。
- `types[]` 是全部 `*Params` 声明（含 `*ParamsBase` / `*ParamsStreaming` /
  `*ParamsNonStreaming`），`fields` 是**合并 `extends` 之后的顶层字段**。
- `coverage.resources` = `methods[].resource` 唯一值数；`coverage.methods` = `methods.length`；
  `coverage.params_types` = `types.length`。`update.sh` 会断言这三条相等。

### `api/SUMMARY.md`

中文、按 resource 分组，每张表列为 方法 / 参数类型 / HTTP / 参数字段数，末尾是汇总。
刻意**不含生成时间**，刷新前后各存一份就能用 `diff` 只看接口变化。

## 收录与排除

收录（52 页）：

- API 参考：`api/messages`、`api/messages/create`、`api/messages/count_tokens`、
  `api/messages/batches{,/create,/list,/retrieve,/results,/cancel,/delete}`、
  `api/models{,/list,/retrieve}`、`api/files{,/upload,/list,/retrieve_metadata,/download,/delete}`。
- API 通用：`overview`、`versioning`、`errors`、`rate-limits`、`beta-headers`、`service-tiers`。
- 核心行为指南：`streaming`、`structured-outputs`（`output_config.format`）、`prompt-caching`、
  `cache-diagnostics`、`thinking`、`extended-thinking`、`thinking-tool-workflows`、
  `thinking-steering-and-cost`、`thinking-troubleshooting`、`preserved-thinking`、
  `token-counting`、`batch-processing`、`files`、`working-with-messages`、`handling-stop-reasons`、
  `build-with-claude/overview`。
- Tool use：`overview`、`define-tools`、`how-tool-use-works`、`handle-tool-calls`、
  `tool-reference`、`parallel-tool-use`、`strict-tool-use`、`fine-grained-tool-streaming`、
  `tool-use-with-prompt-caching`。
- 模型：`models/overview`、`about-claude/pricing`。

排除（要查时直接上官网，或从 SDK `.d.ts` 看类型）：

- `api/beta/**`（beta 命名空间逐端点页，共数百页）——端点与参数在 `api/manifest.json` 的
  `beta.*` resource 与 `.d.ts` 里都有；
- `organization/**` 与 admin（API keys、RBAC、workspaces、spend limits 等组织管理端点）；
- `managed-agents/**`、具体工具页（bash / computer-use / text-editor / web-search / memory 等）、
  `cli-sdks-libraries/**`、`release-notes/**`、逐模型 card 与 system prompts、非英文版本。

## 已知限制与判断

1. **文档只收 52 页**（3.5 MB，远低于 40 MB 上限，所以没有做分片或裁剪压力）。
   选页标准是“实现协议 / 审参数会用到”；排除了整片组织管理与 beta 逐端点页，见上。
2. **没有 HTML 回退实现**。官方 markdown 端点覆盖全部文档，HTML 只是 JS 空壳，
   强行转换只会得到无正文的页面；这是有意的取舍，不是缺口。
3. **GA 与 beta 同名不合并**。`beta.messages.create` 与 `messages.create` 两条都保留，
   因为 resource 路径、返回类型（`BetaMessage` vs `Message`）和可选字段都不同；
   例如 `MessageCreateParamsBase` 在 beta 文件里 28 个顶层字段，GA 只有 19 个
   （`output_format`、`context_management`、`billing_policy` 等只在 beta 侧）。
   同理 `WorkspaceListParams` 等类型在不同 namespace 各声明一次，`types[]` 以
   `(name, file)` 为键各留一条，共 466 条声明 / 349 个唯一名字。
4. **重载合并**：`messages.create` 有 streaming / non-streaming / base 三个签名，合并为一条，
   取类型名的公共前缀（GA 下正好是上游声明的 union alias `MessageCreateParams`）；
   beta 侧没有对应 alias 时退回最后一个（最宽）重载 `MessageCreateParamsBase`。
   `returns` 取最后一个重载的返回类型。
5. **`params` 里包含 header 字段**：SDK 把 `workspace_id`、`betas`、`user_profile_id` 与
   body 字段放在同一个入参对象里，所以字段数会把它们算进来（官方文档是分 header/body 列的）。
6. **路径参数命名不统一**：来自模板字面量的是 `{messageBatchID}`，来自 tracing `route`
   元数据的是 snake_case `{message_batch_id}`。两者都忠于来源，未做归一。
7. **11 个方法 `http: null`**：`messages.stream/parse`、`beta.messages.stream/parse/toolRunner/thread`、
   `beta.sessions.events.toolRunner`、`beta.environments.work.poller/worker`、
   `beta.webhooks.unwrap/parseUnverified` —— 都是 SDK 便捷方法/本地工具，没有唯一的静态端点。
8. **静态解析的边界**：字段只取顶层（嵌套对象类型不展开，只保留类型名）；索引签名
   （`[key: string]: unknown`）不计入字段数；`lib/` 里 10 个零字段类型是纯别名，保留名称、
   字段为空，不省略。
9. `sdk/` 保留全部 `.d.ts`（3.9 MB）而不是只留 `resources/`，因为 `resources` 的类型引用
   `core/`、`lib/`、`internal/` 的声明，删掉会让对照时找不到类型定义。

## 最近一次运行（2026-11-24）

| 项 | 值 |
| --- | --- |
| `./update.sh` | 全流程成功，用时约 56 s（`--sdk-only` 约 2 s；`FORCE=1` 重下 tarball 约 6 s） |
| SDK 版本 | `@anthropic-ai/sdk@1.13.0`（npm latest，抓取于 `2026-11-24T01:36:56Z`） |
| 文档 | 52/52 页成功，正文 3,506,391 B + `llms.txt` 108,297 B = 3,614,688 B；索引 958 条英文页 |
| API 清单 | 117 resources / 399 methods / 466 params 类型声明；其中 388 个方法有静态 HTTP 端点，349 个唯一类型名 |
| 文档生成时间 | `2026-11-24T01:33:08Z` |
| 清单生成时间 | `2026-11-24T01:36:57Z` |

本次校验：

- `bash -n update.sh scripts/fetch-docs.sh scripts/fetch-sdk.sh` 通过；
  `node --check scripts/extract-api.mjs` 通过。
- `update.sh` 自检通过（`coverage` 与数组长度一致）。
- 连续两次运行 `extract-api.mjs`：`manifest.json` 除 `generated_at` 外逐字节一致，
  `SUMMARY.md` 完全一致（校验和相同）。
- `FORCE=1 ./update.sh --sdk-only` 重下同一版本后，`sdk/` 内容与之前逐文件相同
  （仅 `SOURCE` 里的 `fetched_at` 变化），文件数 583、字节数与首次一致。
- 故障路径：在隔离副本里把 `DOCS_BASE_URL` 指向不存在的路径重跑，52 页全部记为
  `status: "failed"` 并带 `curl rc=… http=404` 错误串，`docs/INDEX.md` 追加失败表，
  脚本返回 1（真实快照未被影响）。
- `--docs-only` 不触碰 `api/`，`--sdk-only` 不触碰 `docs/`（前后校验和一致）。
- `git status --short -uall reference/anthropic` 只列出上述生成物与脚本，
  没有 `raw/`、`_tmp/`、`*.tgz`。
