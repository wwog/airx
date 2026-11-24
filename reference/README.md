# reference — 三协议官方行为参照

本目录保存 OpenAI / Anthropic / Gemini 的**官方文档快照**与**官方 npm SDK 的 API 清单**，
用于协议实现对照、参数审查与版本行为分析。它不参与 Cargo 构建，也不是运行时依赖。

“想确认某个参数/接口官方怎么写”时，先看这里的生成物，而不是凭记忆或二手文档。

## 目录结构

```
reference/
├── README.md            # 本文件：约定与刷新流程
├── update-all.sh        # 依次刷新三家
├── openai/
├── anthropic/
└── gemini/
```

每个 provider 目录结构一致：

| 路径 | 内容 | 入库 |
| --- | --- | --- |
| `README.md` | 数据来源、抓取机制、刷新步骤、已知限制 | ✅ |
| `update.sh` | 一条命令刷新 docs + sdk + api；支持 `--docs-only` / `--sdk-only` | ✅ |
| `versions.env` | npm 包名与版本（默认 `latest`，可用环境变量覆盖） | ✅ |
| `scripts/` | 文档抓取、SDK 拉取与 API 提取脚本 | ✅ |
| `GAP.md` | 该协议「官方行为 vs airx 实现」差距分析（人工/agent 撰写，非生成物） | ✅ |
| `docs/` | 官方文档 markdown 快照；`INDEX.md` 记录 URL 清单，`meta.json` 记录抓取时间与结果 | ❌（本地生成） |
| `sdk/<package>@<version>/` | 从 npm tarball 裁剪出的 `.d.ts`、`api.md`、`package.json` 等分析所需文件 | ❌（本地生成） |
| `api/manifest.json` | 全量 API 清单（resource / method / params 类型 / HTTP 路径） | ❌（本地生成） |
| `api/SUMMARY.md` | 人读版 API 清单 | ❌（本地生成） |
| `api/upstream-api.md` | SDK 自带的 `api.md`（若上游提供） | ❌（本地生成） |
| `raw/`、`_tmp/`、`*.tgz` | 下载缓存与解包临时目录 | ❌（本地生成） |

**仓库只提交脚本、说明与分析报告。** 文档、SDK、API 清单都是可复现的本地生成物，一律不入库；
换台机器或需要最新行为时跑一次 `update.sh` 即可。

## 刷新

```bash
cd reference
./update-all.sh                 # 三家全部刷新（默认取 npm latest）

./openai/update.sh              # 单家刷新
./openai/update.sh --docs-only  # 只拉文档
./openai/update.sh --sdk-only   # 只拉 SDK 并重建 API 清单

SDK_VERSION=<version> ./openai/update.sh   # 固定 SDK 版本
```

刷新后：

1. 看 `reference/<provider>/api/SUMMARY.md` 的接口/参数变化（本地生成物不入库，
   需要跨版本对比时先备份该文件再刷新，或直接 diff `sdk/` 里的 `.d.ts`）；
2. 需要细节时读 `docs/` 下对应文档与 `sdk/` 的 `.d.ts`；
3. 仓库里只留脚本与 README；`docs/`、`sdk/`、`api/`、`raw/`、`_tmp/`、`*.tgz`
   都是本地生成物，不要提交。

`update.sh` 是幂等的：重复执行只会覆盖同名快照；版本未变时 `sdk/` 目录不变。

## 约定

- **只读取、不执行**下载的 npm 包内容：脚本仅解析 `.d.ts` / `.md` / `package.json`，
  绝不 `import` / `require` 上游代码，也不运行其安装脚本。
- 文档优先使用官方提供的 markdown / `llms.txt` 入口；只有 HTML 时才降级转换，并在
  各 provider 的 README 里注明保真度限制。
- API 清单必须覆盖 SDK 中声明的**全部** resource / method；无法静态解析的类型在
  `manifest.json` 中保留名称并标注 `unresolved`，不得省略。`types[]` 只放请求参数类型
  （`*Params` / `*Parameters`），其余类型放可选的 `other_types[]`；`coverage` 必须满足
  `resources === 去重 resource 数`、`methods === methods.length`、
  `params_types === types.length`。
- 快照里的时间统一用 UTC ISO-8601（`date -u +%Y-%m-%dT%H:%M:%SZ`），保证 macOS/Linux 一致。

## 为什么不用 git submodule / vendor 整个 SDK

整包体积大且包含运行时实现；这里只保留分析接口所需的类型声明与文档，既能做逐字段
对照，又不会把上游实现副本塞进仓库。需要跑真实请求时用官方 SDK 自行安装，本目录
只作**行为参照**。
