#!/usr/bin/env node
// 从裁剪后的 @google/genai 类型声明中静态提取 API 清单。
//
// 输入（全部按纯文本/纯数据处理，绝不 import / require / eval）：
//   <sdkDir>/dist/genai.d.ts       上游单文件类型声明（interface / class / type / enum）
//   <sdkDir>/dist/http-routes.txt  fetch-sdk.sh 从 dist/index.mjs grep 出的路径模板（可选）
//   <sdkDir>/package.json, VERSION
// 输出：
//   <apiDir>/manifest.json   机器可读全量清单
//   <apiDir>/SUMMARY.md      中文人读版
//   <apiDir>/upstream-api.md 上游自带 api.md 的副本；上游未提供则写指针说明
//
// 用法：node scripts/extract-api.mjs <sdkDir> <apiDir>

import { readFileSync, writeFileSync, existsSync, mkdirSync, copyFileSync } from "node:fs";
import { join, basename } from "node:path";

const [sdkDir, apiDir] = process.argv.slice(2);
if (!sdkDir || !apiDir) {
  console.error("用法: node extract-api.mjs <sdkDir> <apiDir>");
  process.exit(2);
}

const dtsPath = join(sdkDir, "dist", "genai.d.ts");
if (!existsSync(dtsPath)) {
  console.error(`找不到 ${dtsPath}`);
  process.exit(1);
}
mkdirSync(apiDir, { recursive: true });

const raw = readFileSync(dtsPath, "utf8");
const pkg = JSON.parse(readFileSync(join(sdkDir, "package.json"), "utf8"));
const version = existsSync(join(sdkDir, "VERSION"))
  ? readFileSync(join(sdkDir, "VERSION"), "utf8").trim()
  : String(pkg.version || "").trim();
const sourceUrl = "https://registry.npmjs.org/" + pkg.name;
const generatedAt = new Date().toISOString().replace(/\.\d{3}Z$/, "Z");

// ---------------------------------------------------------------------------
// 0. 注释 / 字符串替换为空格（保留换行与偏移，便于按行号定位）
// ---------------------------------------------------------------------------
function sanitize(src) {
  let out = "";
  let i = 0;
  const n = src.length;
  while (i < n) {
    const c = src[i];
    const d = src[i + 1];
    if (c === "/" && d === "/") {
      let j = src.indexOf("\n", i);
      if (j < 0) j = n;
      out += " ".repeat(j - i);
      i = j;
      continue;
    }
    if (c === "/" && d === "*") {
      let j = src.indexOf("*/", i + 2);
      j = j < 0 ? n : j + 2;
      for (let k = i; k < j; k++) out += src[k] === "\n" ? "\n" : " ";
      i = j;
      continue;
    }
    if (c === '"' || c === "'" || c === "`") {
      const q = c;
      let j = i + 1;
      while (j < n && src[j] !== q) {
        if (src[j] === "\\") j++;
        j++;
      }
      j = Math.min(j + 1, n);
      for (let k = i; k < j; k++) out += src[k] === "\n" ? "\n" : " ";
      i = j;
      continue;
    }
    out += c;
    i++;
  }
  return out;
}
const san = sanitize(raw);
const sanLines = san.split("\n");

function matchBrace(s, openIdx) {
  if (s[openIdx] !== "{") return -1;
  let depth = 0;
  for (let j = openIdx; j < s.length; j++) {
    const ch = s[j];
    if (ch === "{") depth++;
    else if (ch === "}") {
      depth--;
      if (depth === 0) return j;
    }
  }
  return -1;
}

function lineStarts(s) {
  const starts = [0];
  for (let i = 0; i < s.length; i++) if (s[i] === "\n") starts.push(i + 1);
  return starts;
}
const starts = lineStarts(san);
function lineOf(offset) {
  let lo = 0;
  let hi = starts.length - 1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (starts[mid] <= offset) lo = mid;
    else hi = mid - 1;
  }
  return lo + 1;
}

// ---------------------------------------------------------------------------
// 1. 顶层声明索引
// ---------------------------------------------------------------------------
const decls = new Map(); // name -> decl
const declList = [];
const declRe = /^(export )?declare (class|interface|enum|type|namespace)\s+([A-Za-z0-9_$]+)/;

for (let i = 0; i < sanLines.length; i++) {
  const m = sanLines[i].match(declRe);
  if (!m) continue;
  const kind = m[2];
  const name = m[3];
  const exported = Boolean(m[1]);
  const lineOffset = starts[i];
  let bodyStart = -1;
  let bodyEnd = -1;
  if (kind === "type") {
    const eq = san.indexOf("=", lineOffset);
    if (eq < 0) continue;
    // 找到深度 0 的 ';'
    let depth = 0;
    let j = eq + 1;
    for (; j < san.length; j++) {
      const ch = san[j];
      if (ch === "{" || ch === "(" || ch === "[") depth++;
      else if (ch === "}" || ch === ")" || ch === "]") depth--;
      else if (ch === ";" && depth === 0) break;
    }
    bodyStart = eq + 1;
    bodyEnd = j;
  } else {
    const brace = san.indexOf("{", lineOffset);
    if (brace < 0) continue;
    bodyStart = brace;
    bodyEnd = matchBrace(san, brace);
    if (bodyEnd < 0) continue;
  }
  const declaration = {
    kind,
    name,
    exported,
    bodyStart,
    bodyEnd,
    line: lineOf(lineOffset),
    header: san.slice(lineOffset, bodyStart).replace(/\s+/g, " ").trim(),
  };
  declList.push(declaration);
  if (!decls.has(name)) decls.set(name, declaration);
}

function bodyOf(decl) {
  const from = decl.kind === "type" ? decl.bodyStart : decl.bodyStart + 1;
  return san.slice(from, decl.bodyEnd);
}

// 顶层声明之外的命名空间别名：export type X = Y; / export interface X extends Y {}
const namespaceAliases = new Map(); // "Ns.Name" -> target name
for (const d of declList) {
  if (d.kind !== "namespace") continue;
  const body = bodyOf(d);
  const re = /export\s+(?:type|interface)\s+([A-Za-z0-9_$]+)\s*(?:=[^;]*?([A-Za-z0-9_$]+)\s*;|extends\s+([A-Za-z0-9_$]+))/g;
  let m;
  while ((m = re.exec(body))) {
    const target = (m[2] || m[3] || "").trim();
    if (target) namespaceAliases.set(`${d.name}.${m[1]}`, target);
  }
}
// 普通 re-export 命名空间（export { A, B }），用于 types.X -> X 归一
const reExportNamespaces = new Set();
for (const d of declList) {
  if (d.kind !== "namespace") continue;
  if (/export\s*\{/.test(bodyOf(d))) reExportNamespaces.add(d.name);
}

// ---------------------------------------------------------------------------
// 2. 成员解析（interface / class 体在 depth 1 的字段与方法）
// ---------------------------------------------------------------------------
function splitTopLevel(text, sep) {
  const parts = [];
  let depth = 0;
  let cur = "";
  for (let i = 0; i < text.length; i++) {
    const ch = text[i];
    if (ch === "{" || ch === "(" || ch === "[" || ch === "<") depth++;
    else if (ch === "}" || ch === ")" || ch === "]" || ch === ">") depth--;
    if (ch === sep && depth === 0) {
      parts.push(cur);
      cur = "";
      continue;
    }
    cur += ch;
  }
  parts.push(cur);
  return parts;
}

// 把 body 切成 depth 1 的声明片段：以 depth 0 的 ';' 为界（body 内部深度从 0 起算）
function memberSegments(body) {
  const segments = [];
  let depth = 0;
  let cur = "";
  for (let i = 0; i < body.length; i++) {
    const ch = body[i];
    if (ch === "{" || ch === "(" || ch === "[") depth++;
    else if (ch === "}" || ch === ")" || ch === "]") depth--;
    if (ch === ";" && depth === 0) {
      segments.push(cur);
      cur = "";
      continue;
    }
    cur += ch;
  }
  if (cur.trim()) segments.push(cur);
  return segments;
}

function cleanType(t) {
  return t
    .replace(/\s+/g, " ")
    .replace(/\s*\|\s*null\s*\|\s*undefined\s*$/, "")
    .replace(/\s*\|\s*undefined\s*$/, "")
    .replace(/\s*\|\s*null\s*$/, "")
    .trim();
}

const PRIMITIVES = new Set(["string", "number", "boolean", "void", "unknown", "any", "undefined", "null", "Uint8Array", "object"]);

function looksLikeTypeRef(typeText) {
  const base = typeText.replace(/<[\s\S]*$/, "").trim();
  if (PRIMITIVES.has(base)) return false;
  if (/^(?:\{|\[|\(|')/.test(base)) return false;
  return /^[A-Za-z_$][\w$]*(?:\.[A-Za-z_$][\w$]*)?$/.test(base);
}

function pickParamsType(argList) {
  const parts = splitTopLevel(argList, ",").map((s) => s.trim()).filter(Boolean);
  const parsed = [];
  for (const p of parts) {
    const m = p.match(/^([A-Za-z0-9_$]+)\s*(\?)?\s*:\s*([\s\S]+)$/);
    if (m) parsed.push({ name: m[1], optional: Boolean(m[2]), type: m[3] });
  }
  for (const want of ["params", "parameters", "body", "request"]) {
    const hit = parsed.find((p) => p.name === want);
    if (hit) return hit.type;
  }
  const typed = parsed.find((p) => looksLikeTypeRef(p.type.trim()));
  return typed ? typed.type : null;
}

function parseMembers(body) {
  const fields = [];
  const methods = [];
  for (const segRaw of memberSegments(body)) {
    let seg = segRaw.trim();
    if (!seg) continue;
    seg = seg.replace(/^(?:public|protected|private|static|readonly|abstract|declare)\s+/g, "");
    if (/^constructor\b/.test(seg)) continue;
    if (/^\[/.test(seg)) continue; // index signature

    // getter / setter
    const getter = seg.match(/^(get|set)\s+([A-Za-z0-9_$]+)\s*\(\s*\)\s*:\s*([\s\S]+)$/);
    if (getter) {
      if (getter[1] === "get") methods.push({ name: getter[2], args: "", returns: cleanType(getter[3]), accessor: true });
      continue;
    }

    // 方法： name<T>(args): ret
    const method = seg.match(/^([A-Za-z0-9_$]+)\s*(<[^(]*?>)?\s*\(([\s\S]*)\)\s*:\s*([\s\S]+)$/);
    if (method) {
      methods.push({ name: method[1], args: method[3], returns: cleanType(method[4]) });
      continue;
    }

    // 属性： name?: type  （或函数类型 name: (args) => ret）
    const prop = seg.match(/^([A-Za-z0-9_$]+)\s*(\?)?\s*:\s*([\s\S]+)$/);
    if (prop) {
      const typeText = cleanType(prop[3]);
      const fn = typeText.match(/^\(([\s\S]*)\)\s*=>\s*([\s\S]+)$/);
      if (fn) {
        methods.push({ name: prop[1], args: fn[1], returns: cleanType(fn[2]) });
      } else {
        fields.push({ name: prop[1], optional: Boolean(prop[2]), type: typeText });
      }
      continue;
    }
  }
  return { fields, methods };
}

function declFields(decl) {
  if (!decl) return [];
  if (decl.kind === "interface" || decl.kind === "class") return parseMembers(bodyOf(decl)).fields;
  if (decl.kind === "type") {
    const body = bodyOf(decl).trim();
    // 联合类型（A | B | {..}）不做字段合并，避免把多个分支的字段混在一起
    if (splitTopLevel(body, "|").length > 1) return [];
    if (body.startsWith("{")) return parseMembers(body.slice(1, -1)).fields;
    return [];
  }
  if (decl.kind === "enum") {
    return bodyOf(decl)
      .split(",")
      .map((s) => s.trim())
      .filter((s) => /^[A-Za-z0-9_$]+\s*=/.test(s))
      .map((s) => s.replace(/\s*=\s*[\s\S]*$/, ""));
  }
  return [];
}

// ---------------------------------------------------------------------------
// 3. 类型名归一
// ---------------------------------------------------------------------------
function canonicalType(expr) {
  if (!expr) return null;
  let t = expr.trim();
  // Promise<X> / APIPromise<X> 解包
  let guard = 0;
  while (/^(?:APIPromise|Promise)</.test(t) && guard++ < 5) {
    const inner = t.replace(/^(?:APIPromise|Promise)</, "");
    let depth = 1;
    let end = 0;
    for (let i = 0; i < inner.length; i++) {
      if (inner[i] === "<") depth++;
      else if (inner[i] === ">") {
        depth--;
        if (depth === 0) {
          end = i;
          break;
        }
      }
    }
    if (!end) break;
    t = inner.slice(0, end).trim();
  }
  // 去掉命名空间前缀（types./Interactions./Triggers./...）
  const replaceQualified = (s) =>
    s.replace(/\b([A-Za-z_$][\w$]*)\.([A-Za-z_$][\w$]*)/g, (full, ns, name) => {
      if (ns === "types" || reExportNamespaces.has(ns)) return name;
      if (namespaceAliases.has(`${ns}.${name}`)) return `${ns}.${name}`;
      return name;
    });
  t = replaceQualified(t);
  return cleanType(t);
}

// 解析类型引用到实际声明（跟随别名链，最多 6 跳）
function resolveDecl(name) {
  let cur = name;
  for (let i = 0; i < 6; i++) {
    if (decls.has(cur) && decls.get(cur).kind !== "type") return decls.get(cur);
    const alias = namespaceAliases.get(cur);
    if (alias) {
      cur = alias;
      continue;
    }
    const d = decls.get(cur);
    if (d && d.kind === "type") {
      const rhs = bodyOf(d).trim();
      const simple = rhs.match(/^([A-Za-z_$][\w$]*)$/);
      if (simple && simple[1] !== cur) {
        cur = simple[1];
        continue;
      }
      const dotted = rhs.match(/^([A-Za-z_$][\w$]*\.[A-Za-z_$][\w$]*)$/);
      if (dotted && dotted[1] !== cur) {
        cur = dotted[1];
        continue;
      }
      return d;
    }
    return d || null;
  }
  return decls.get(cur) || null;
}

function fieldsOfTypeName(name) {
  if (!name) return null;
  const d = resolveDecl(name.replace(/<[\s\S]*$/, "").trim());
  if (!d) return null;
  const f = declFields(d);
  return f.length ? f : [];
}

// ---------------------------------------------------------------------------
// 4. 资源（namespace/class）发现
// ---------------------------------------------------------------------------
const resourceClasses = []; // {resource, className, note}
function addResource(resource, className, note) {
  if (!className || !decls.has(className)) return;
  if (resourceClasses.some((r) => r.resource === resource && r.className === className)) return;
  resourceClasses.push({ resource, className, note: note || null });
}

const root = decls.get("GoogleGenAI");
if (root) {
  const body = bodyOf(root);
  for (const line of body.split("\n")) {
    const prop = line.match(/^\s*readonly\s+([A-Za-z0-9_$]+)\s*:\s*([A-Za-z0-9_$]+)\s*;/);
    if (prop) addResource(prop[1], prop[2]);
    const getter = line.match(/^\s*get\s+([A-Za-z0-9_$]+)\s*\(\s*\)\s*:\s*([A-Za-z0-9_$]+)/);
    if (getter) addResource(getter[1], getter[2]);
  }
}

// 资源内部挂载的子资源（从类体里的 readonly 字段发现）
const nestedNotes = {
  documents: "FileSearchStores 下的子资源（ai.fileSearchStores.documents）",
  music: "Live 的音乐会话子资源（ai.live.music）",
  files: "Environments 下的文件子资源",
  internal: "Environments 内部上传入口",
  auth_tokens: "Interactions 的临时 token 子资源",
};
const seen = new Set(resourceClasses.map((r) => `${r.resource}::${r.className}`));
for (const r of [...resourceClasses]) {
  const d = decls.get(r.className);
  if (!d) continue;
  for (const line of bodyOf(d).split("\n")) {
    const m = line.match(/^\s*(?:readonly|get)\s+([A-Za-z0-9_$]+)\s*(?::|\(\s*\)\s*:)\s*([A-Za-z0-9_$]+)/);
    if (!m) continue;
    const sub = `${r.resource}.${m[1]}`;
    const key = `${sub}::${m[2]}`;
    if (seen.has(key)) continue;
    if (!decls.has(m[2])) continue;
    seen.add(key);
    resourceClasses.push({ resource: sub, className: m[2], note: nestedNotes[m[1]] || "子资源" });
  }
}

// 返回值对象（Chat / Session）也当作方法集合收录
const returnedResources = [
  { resource: "chat", className: "Chat", note: "chats.create() 返回的会话对象" },
  { resource: "live.session", className: "Session", note: "live.connect() 返回的会话对象" },
];
for (const r of returnedResources) {
  if (decls.has(r.className) && !resourceClasses.some((x) => x.resource === r.resource)) {
    resourceClasses.push(r);
  }
}

// ---------------------------------------------------------------------------
// 5. HTTP 路径（来自 fetch-sdk.sh 生成的 dist/http-routes.txt，纯静态 grep 结果）
//    路由文件保留四类行：`class X` / `let X$1 = class X` 边界、`function name(` 边界、
//    `formatMap('template')` 或 `pathToFunc("/{api_version}/...")` 路径模板、
//    `httpMethod: 'VERB'` 或 `method: "VERB"` 动词。
//    两套实现风格分别对应「BaseModule 类的内部方法」和「顶层 XxxCreate 请求构造器」。
// ---------------------------------------------------------------------------
const routeFile = join(sdkDir, "dist", "http-routes.txt");
const containers = new Map(); // key -> {kind:'class'|'fn', name, fns: Map<fnName, route[]>}
let httpAvailable = false;
if (existsSync(routeFile)) {
  httpAvailable = true;
  let cur = null;
  let curFn = null;
  let pending = null;
  const ensure = (key, kind, name) => {
    if (!containers.has(key)) containers.set(key, { kind, name, fns: new Map() });
    return containers.get(key);
  };
  for (const line of readFileSync(routeFile, "utf8").split("\n")) {
    if (!line || line.startsWith("#")) continue;
    const m = line.match(/^\d+:(.*)$/);
    if (!m) continue;
    const text = m[1].slice(1); // 去掉 "NR: " 的分隔空格，保留源码缩进

    const classDecl = text.match(/^(?:let )?([A-Za-z0-9_$]+) = class ([A-Za-z0-9_$]+)/);
    const plainClass = text.match(/^class ([A-Za-z0-9_$]+)/);
    if (classDecl || plainClass) {
      const key = classDecl ? classDecl[1] : plainClass[1];
      const name = classDecl ? classDecl[2] : plainClass[1];
      cur = ensure(key, "class", name);
      curFn = null;
      pending = null;
      continue;
    }

    const topFn = text.match(/^function ([A-Za-z0-9_$]+)\(/);
    if (topFn) {
      // esbuild 把真正的请求体放在 `$do$xx` 里，紧跟其包装函数，视为同一段
      if (!topFn[1].startsWith("$do$")) {
        cur = ensure(`fn:${topFn[1]}`, "fn", topFn[1]);
        cur.fns.set(topFn[1], []);
        curFn = topFn[1];
        pending = null;
      }
      continue;
    }

    const member = text.match(/^    (?:async )?([A-Za-z0-9_$]+)\((?:params|parameters)[,)]/);
    if (member && cur && cur.kind === "class") {
      curFn = member[1];
      if (!cur.fns.has(curFn)) cur.fns.set(curFn, []);
      pending = null;
      continue;
    }

    const tpl = text.match(/formatMap\('([^']+)'/) || text.match(/pathToFunc\("([^"]+)"/);
    if (tpl) {
      pending = tpl[1];
      continue;
    }
    // endpointUrl = '{model}:embedContent' 这类「同一方法多个候选端点」的赋值
    const extra = text.match(/^\s*(?:let |const )?[A-Za-z0-9_$]+ = '([^']+)';\s*$/);
    if (extra && cur && curFn) {
      if (!cur.extras) cur.extras = new Map();
      if (!cur.extras.has(curFn)) cur.extras.set(curFn, []);
      cur.extras.get(curFn).push(extra[1]);
      continue;
    }
    const verb = text.match(/httpMethod:\s*'([A-Z]+)'/) || text.match(/\bmethod:\s*"([A-Z]+)"/);
    if (verb && pending && cur && curFn) {
      cur.fns.get(curFn).push({ verb: verb[1], template: pending });
      pending = null;
    }
  }
}

// Vertex 专属模板：同组里有 Gemini API 模板时优先后者
const vertexOnly = [/batchPredictionJobs/, /^tuningJobs/, /:fetchPredictOperation/, /^\{model\}:predict$/, /^\{model\}:predictLongRunning$/];

function templateToPath(template) {
  let t = template;
  t = t.replace("{api_version}", "v1beta");
  t = t.replace("{models_url}", "models");
  t = t.replace("{model}", "models/{model}");
  if (!t.startsWith("/")) {
    if (t.startsWith("upload/")) return "/" + t;
    return "/v1beta/" + t;
  }
  return t.replace("/{api_version}", "/v1beta");
}

function routeToHttp(routes, methodName, extras) {
  if (!routes || !routes.length) return null;
  const gemini = routes.filter((r) => !vertexOnly.some((re) => re.test(r.template)));
  const pool = gemini.length ? gemini : routes;
  const all = [...pool];
  for (const t of extras || []) {
    if (!vertexOnly.some((re) => re.test(t))) all.push({ verb: pool[0].verb, template: t });
  }
  const uniq = [];
  const seenPaths = new Set();
  for (const r of all) {
    const path = templateToPath(r.template);
    const key = `${r.verb} ${path}`;
    if (seenPaths.has(key)) continue;
    seenPaths.add(key);
    uniq.push(`${r.verb} ${path}`);
  }
  // 若模板体现了方法名（如 :generateContent / :embedContent），优先精确匹配
  const exact = methodName ? uniq.find((u) => new RegExp(`:${methodName}(\\?|$)`).test(u)) : null;
  const primary = exact || uniq[0];
  const alternatives = uniq.filter((u) => u !== primary);
  return { http: primary, alternatives };
}

// 同一函数名出现在多个容器里时，只有唯一归属才全局兜底
const fnOwners = new Map(); // fnName -> Set(containerKey)
for (const [key, c] of containers) {
  for (const fn of c.fns.keys()) {
    if (!fnOwners.has(fn)) fnOwners.set(fn, new Set());
    fnOwners.get(fn).add(key);
  }
}

const fnContainers = new Map(); // 顶层请求构造器名 -> container
for (const [key, c] of containers) if (c.kind === "fn") fnContainers.set(c.name, c);

// 类型声明里看不到、但 bundle 中以字符串常量传给 ApiClient 的端点（人工核对后补充）
const manualHttp = {
  "files.upload": { http: "POST /upload/v1beta/files", note: "resumable upload，路径常量见 ApiClient.uploadFile" },
  "operations.get": { http: "GET /v1beta/{name}", note: "长任务 operation 资源名（bundled 的通用 LRO getter）" },
};

function candidatesForClass(dtsName) {
  const names = new Set([dtsName, dtsName.replace(/^GeminiNextGen/, ""), dtsName.replace(/_2$/, "")]);
  const out = [];
  for (const [key, c] of containers) {
    if (c.kind !== "class") continue;
    if (names.has(c.name) || names.has(key)) out.push([key, c]);
  }
  return out;
}

function pickBundleClass(dtsName, methodNames) {
  const cands = candidatesForClass(dtsName);
  let best = null;
  let bestScore = 0;
  for (const [key, c] of cands) {
    let score = 0;
    for (const m of methodNames) {
      if (c.fns.has(m) && c.fns.get(m).length) score++;
      else if (c.fns.has(`${m}Internal`) && c.fns.get(`${m}Internal`).length) score++;
    }
    if (score > bestScore) {
      best = { key, cls: c, score };
      bestScore = score;
    }
  }
  return best;
}

// 资源复数 -> 单数（顶层请求构造器命名用，如 environmentsCreateEnvironment）
const singularMap = {
  agents: "Agent",
  triggers: "Trigger",
  webhooks: "Webhook",
  credentials: "Credential",
  decisions: "Decision",
  environments: "Environment",
  voices: "Voice",
  interactions: "Interaction",
  "interactions.authTokens": "InteractionAuthToken",
  authTokens: "AuthToken",
  models: "Model",
  files: "File",
  caches: "CachedContent",
  batches: "Batch",
  tunings: "TuningJob",
  operations: "Operation",
  fileSearchStores: "FileSearchStore",
  "fileSearchStores.documents": "Document",
};

// ---------------------------------------------------------------------------
// 6. 汇总方法
// ---------------------------------------------------------------------------
const methods = [];
for (const r of resourceClasses) {
  const d = decls.get(r.className);
  if (!d) continue;
  const { methods: members } = parseMembers(bodyOf(d));
  const callable = members.filter((m) => !m.accessor);
  const bundle = pickBundleClass(r.className, callable.map((m) => m.name));

  for (const m of callable) {
    const paramsRaw = m.args ? pickParamsType(m.args) : null;
    const params = canonicalType(paramsRaw);
    let http = null;
    let httpSource = null;
    let httpAlternatives = [];
    let httpNote = null;
    if (bundle) {
      for (const k of [m.name, `${m.name}Internal`, `_${m.name}`]) {
        const routes = bundle.cls.fns.get(k);
        if (routes && routes.length) {
          const r2 = routeToHttp(routes, m.name, bundle.cls.extras && bundle.cls.extras.get(k));
          http = r2.http;
          httpAlternatives = r2.alternatives;
          httpSource = "sdk-bundle";
          break;
        }
      }
    }
    // 顶层请求构造器命名：<resourceCamel><MethodCap>（如 triggersCreate / interactionsAuthTokensDelete）
    if (!http) {
      const camel = r.resource
        .split(".")
        .map((seg, i) => (i === 0 ? seg.charAt(0).toLowerCase() + seg.slice(1) : seg.charAt(0).toUpperCase() + seg.slice(1)))
        .join("");
      const cap = m.name.charAt(0).toUpperCase() + m.name.slice(1);
      const singular = singularMap[r.resource] || "";
      const plural = singular ? `${singular}s` : null;
      const names = [
        `${camel}${cap}`,
        singular ? `${camel}${cap}${singular}` : null,
        singular ? `${camel}${singular}${cap}` : null,
        plural ? `${camel}${cap}${plural}` : null,
      ].filter(Boolean);
      for (const n of names) {
        const builder = fnContainers.get(n);
        if (!builder) continue;
        const routes = builder.fns.get(builder.name);
        if (routes && routes.length) {
          const r2 = routeToHttp(routes, m.name, builder.extras && builder.extras.get(builder.name));
          http = r2.http;
          httpAlternatives = r2.alternatives;
          httpSource = "sdk-bundle";
          break;
        }
      }
    }
    if (!http) {
      const manual = manualHttp[`${r.resource}.${m.name}`];
      if (manual) {
        http = manual.http;
        httpSource = "manual";
        httpNote = manual.note || httpNote;
      }
    }
    const entry = {
      resource: r.resource,
      method: m.name,
      params: params || null,
      returns: canonicalType(m.returns),
      http: http || null,
      http_alternatives: httpAlternatives.length ? httpAlternatives : undefined,
      http_note: httpNote || (http && /^[A-Z]+ \/v1beta\/\{name\}(:.*)?$/.test(http) ? "{name} 为完整资源名（如 models/<id>、cachedContents/<id>）" : undefined),
      http_source: httpSource || undefined,
      params_resolvable: Boolean(params && resolveDecl(params.replace(/<[\s\S]*$/, "").trim())),
      optional_params: /^\s*params\s*\?/i.test(m.args || "") || undefined,
      note: r.note || undefined,
    };
    for (const k of Object.keys(entry)) if (entry[k] === undefined) delete entry[k];
    methods.push(entry);
  }
}

// 去重（同一 resource+method 的多次重载只保留签名最完整的一条）
const mergedMethods = [];
const methodIndex = new Map();
for (const m of methods) {
  const key = `${m.resource}.${m.method}`;
  const prev = methodIndex.get(key);
  if (!prev) {
    methodIndex.set(key, m);
    mergedMethods.push(m);
    continue;
  }
  const score = (x) => (x.params_resolvable ? 6 : 0) + (x.params ? 2 : 0) + (x.returns ? 1 : 0);
  if (score(m) > score(prev)) {
    const idx = mergedMethods.indexOf(prev);
    mergedMethods[idx] = m;
    methodIndex.set(key, m);
  } else if (m.params && !prev.params) {
    prev.params = m.params;
  }
}
for (const m of mergedMethods) delete m.class;
mergedMethods.sort((a, b) => (a.resource < b.resource ? -1 : a.resource > b.resource ? 1 : a.method < b.method ? -1 : 1));

// ---------------------------------------------------------------------------
// 7. 类型清单
// ---------------------------------------------------------------------------
const types = [];
const emitted = new Set();
function pushType(entry) {
  if (emitted.has(entry.name)) return;
  emitted.add(entry.name);
  types.push(entry);
}

for (const d of declList) {
  if (d.kind === "namespace") continue;
  if (d.name.endsWith("$")) continue; // 生成器的内部原名，通过 namespace 别名以公开名收录
  let fields = declFields(d);
  const rhs = d.kind === "type" ? bodyOf(d).trim() : "";
  const aliasTarget = d.kind === "type" && /^[A-Za-z_$][\w$]*(?:\.[A-Za-z_$][\w$]*)?$/.test(rhs) ? rhs : null;
  if (d.kind === "type" && aliasTarget) {
    // 跟随别名链取字段（如 Interactions.Content -> Content$ -> Content_2 取不到则该别名无字段）
    const followed = fieldsOfTypeName(d.name);
    if (followed && followed.length) fields = followed;
  }
  const entry = {
    name: d.name,
    kind: d.kind,
    file: "dist/genai.d.ts",
    line: d.line,
    exported: d.exported,
    fields: fields.map((f) => `${f.name}${f.optional ? "?" : ""}: ${f.type}`),
  };
  if (aliasTarget) entry.alias = aliasTarget;
  if (aliasTarget && !fields.length) {
    entry.unresolved = true;
  } else if (d.kind === "type" && !fields.length) {
    entry.alias = rhs.replace(/\s+/g, " ").slice(0, 240);
    entry.unresolved = true;
  } else if ((d.kind === "interface" || d.kind === "class") && !fields.length && bodyOf(d).trim().length > 0) {
    entry.unresolved = true;
  }
  pushType(entry);
}

// namespace 别名：以公开名收录（Agents.Agent / Interactions.Content / Triggers.Trigger ...）
const emittedAliases = [];
for (const [qualified, target] of namespaceAliases) {
  const nsName = qualified.split(".")[0];
  const aliasDecl = declList.find((d) => d.kind === "namespace" && d.name === nsName);
  if (!aliasDecl || !aliasDecl.exported) continue; // 只收公开命名空间
  const entry = {
    name: qualified,
    kind: "alias",
    file: "dist/genai.d.ts",
    line: aliasDecl.line,
    exported: true,
    target,
    fields: [],
  };
  const f = fieldsOfTypeName(target);
  if (f && f.length) entry.fields = f.map((x) => `${x.name}${x.optional ? "?" : ""}: ${x.type}`);
  else entry.unresolved = true;
  if (emitted.has(entry.name)) continue;
  emitted.add(entry.name);
  emittedAliases.push(entry);
}
types.push(...emittedAliases);
types.sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0));

// ---------------------------------------------------------------------------
// 8. 统计与一致性校验
// ---------------------------------------------------------------------------
const resources = [...new Set(mergedMethods.map((m) => m.resource))].sort();

// types[] 只放「请求参数类型」（*Parameters / *Params），与 openai/anthropic 两家的清单口径一致：
//   coverage.params_types === types.length
// 其余声明（响应体、枚举、工具类型等）放在 other_types[]，也能按名字交叉检索。
const isParamsType = (t) => /Parameters$|Params$/.test(t.name);
const paramsTypes = types.filter(isParamsType);
const otherTypes = types.filter((t) => !isParamsType(t));
const allTypes = types;
const coverage = {
  resources: resources.length,
  methods: mergedMethods.length,
  params_types: paramsTypes.length,
  other_types: otherTypes.length,
  types_total: allTypes.length,
};
const withHttp = mergedMethods.filter((m) => m.http).length;
const unresolvedParams = paramsTypes.filter((t) => t.unresolved).length;
const unresolvedOther = otherTypes.filter((t) => t.unresolved).length;
const unresolved = unresolvedParams + unresolvedOther;

function assertCoverage() {
  const problems = [];
  if (coverage.methods !== mergedMethods.length) problems.push("methods 计数与数组长度不一致");
  if (coverage.resources !== new Set(mergedMethods.map((m) => m.resource)).size) problems.push("resources 计数与去重资源数不一致");
  // 共享契约：params_types 必须等于 types[] 长度（同日/同源计数）
  if (coverage.params_types !== paramsTypes.length) problems.push("params_types 计数与 types[] 长度不一致");
  if (coverage.other_types !== otherTypes.length) problems.push("other_types 计数与 other_types[] 长度不一致");
  if (coverage.types_total !== paramsTypes.length + otherTypes.length) problems.push("types_total 不等于 params_types + other_types");
  if (coverage.params_types > coverage.types_total) problems.push("params_types 大于 types_total");
  const dup = new Set();
  for (const m of mergedMethods) {
    const k = `${m.resource}.${m.method}`;
    if (dup.has(k)) problems.push(`重复方法 ${k}`);
    dup.add(k);
  }
  if (problems.length) {
    console.error("一致性校验失败:\n - " + problems.join("\n - "));
    process.exit(1);
  }
}
assertCoverage();

const manifest = {
  provider: "gemini",
  package: pkg.name,
  version,
  generated_at: generatedAt,
  source: sourceUrl,
  http_source: httpAvailable ? "derived from dist/index.mjs HTTP path templates (static grep, code not executed)" : "unavailable",
  types_note: "types[] 仅含 *Parameters/*Params 请求参数类型（coverage.params_types === types.length）；其余声明在 other_types[]（coverage.other_types），两者合计 coverage.types_total。",
  coverage,
  methods: mergedMethods,
  types: paramsTypes,
  other_types: otherTypes,
};
writeFileSync(join(apiDir, "manifest.json"), JSON.stringify(manifest, null, 2) + "\n");

// ---------------------------------------------------------------------------
// 9. SUMMARY.md（中文）
// ---------------------------------------------------------------------------
function mdEscape(s) {
  return String(s == null ? "" : s).replace(/\|/g, "\\|");
}
function paramCount(m) {
  const f = fieldsOfTypeName(m.params);
  return f ? f.length : 0;
}

const lines = [];
lines.push("# Gemini API 清单（`@google/genai`）");
lines.push("");
lines.push(`- 包：\`${pkg.name}@${version}\``);
lines.push(`- 生成时间（UTC）：${generatedAt}`);
lines.push(`- 来源：${sourceUrl}`);
lines.push(`- 生成脚本：\`scripts/extract-api.mjs\`（静态解析 \`dist/genai.d.ts\`，不执行上游代码）`);
lines.push("");
lines.push("## 总计");
lines.push("");
lines.push("| 指标 | 数量 |");
lines.push("| --- | ---: |");
lines.push(`| 资源（resource） | ${coverage.resources} |`);
lines.push(`| 方法（method） | ${coverage.methods} |`);
lines.push(`| 方法中有 HTTP 端点的 | ${withHttp}（sdk-bundle ${mergedMethods.filter((m) => m.http_source === "sdk-bundle").length} / manual ${mergedMethods.filter((m) => m.http_source === "manual").length}） |`);
lines.push(`| 请求参数类型 \`types[]\`（\\*Parameters / \\*Params） | ${coverage.params_types} |`);
lines.push(`| 其余类型 \`other_types[]\`（响应体/枚举/工具类型等） | ${coverage.other_types} |`);
lines.push(`| 类型合计（types_total） | ${coverage.types_total} |`);
lines.push(`| 未能静态解析（unresolved） | ${unresolved}（types[] ${unresolvedParams} / other_types[] ${unresolvedOther}） |`);
lines.push("");
lines.push("> HTTP 列由 SDK bundle 里的 `formatMap('<template>')` / `pathToFunc()` + `httpMethod` / `method` 静态提取，路径相对 `https://generativelanguage.googleapis.com`，默认 `v1beta`。");
lines.push("> `{model}` 为模型 id（SDK 拼成 `models/<id>`）；`{name}` / `{parent}` / `{file}` / `{operationName}` 为资源全名占位符（`{name}` 通常已包含 `models/`、`cachedContents/` 前缀）。`alt:` 表示同一方法的另一条候选路径（如 embedContent 单条 vs 批量）。`—` 表示该方法不发 REST 请求（本地包装、WebSocket、上传/下载实现），没有可静态提取的路径。");
lines.push("");
lines.push("## 按资源分组");
lines.push("");
for (const r of resources) {
  const group = mergedMethods.filter((m) => m.resource === r);
  const note = group.find((m) => m.note)?.note;
  lines.push(`### ${r} （${group.length} 个方法）`);
  lines.push("");
  if (note) {
    lines.push(`> ${note}`);
    lines.push("");
  }
  lines.push("| 方法 | params 类型 | HTTP | 参数字段数 | 返回类型 |");
  lines.push("| --- | --- | --- | ---: | --- |");
  for (const m of group) {
    lines.push(
      `| \`${m.method}\` | ${m.params ? "`" + mdEscape(m.params) + "`" : "—"} | ${m.http ? "`" + mdEscape(m.http) + "`" + (m.http_alternatives ? "<br>alt: " + m.http_alternatives.map((a) => "`" + mdEscape(a) + "`").join("、") : "") : "—"} | ${paramCount(m)} | ${m.returns ? "`" + mdEscape(m.returns) + "`" : "—"} |`,
    );
  }
  lines.push("");
}

lines.push("## 类型条目索引");
lines.push("");
lines.push("`manifest.json` 的 `types[]` 只放请求参数类型（`*Parameters` / `*Params`，共 " + paramsTypes.length + " 条），与 openai / anthropic 的清单口径一致（`coverage.params_types === types.length`）；响应体、枚举、工具类型等放在 `other_types[]`（" + otherTypes.length + " 条）。两者合计 " + allTypes.length + " 条，每项都含 `fields`，形如 `字段名?: 类型`。");
lines.push("");
lines.push("### 请求参数类型（`types[]`，共 " + paramsTypes.length + " 条）");
lines.push("");
lines.push("| 类型 | kind | 字段数 | 所在行 |");
lines.push("| --- | --- | ---: | ---: |");
for (const t of paramsTypes) {
  lines.push(`| \`${t.name}\` | ${t.kind} | ${t.fields.length}${t.unresolved ? " (unresolved)" : ""} | ${t.line} |`);
}
lines.push("");
lines.push("### 其余类型按 kind 统计（`other_types[]`）");
lines.push("");
const kindCounts = allTypes.reduce((acc, t) => {
  acc[t.kind] = (acc[t.kind] || 0) + 1;
  return acc;
}, {});
lines.push("| kind | 数量 |");
lines.push("| --- | ---: |");
for (const k of Object.keys(kindCounts).sort()) lines.push(`| ${k} | ${kindCounts[k]} |`);
lines.push("");
lines.push("重点类型（generateContent 主线，取自 `types[]` + `other_types[]`）：");
lines.push("");
const keyTypes = [
  "GenerateContentParameters",
  "GenerateContentConfig",
  "GenerateContentResponse",
  "EmbedContentParameters",
  "EmbedContentConfig",
  "CountTokensParameters",
  "CreateCachedContentParameters",
  "CreateCachedContentConfig",
  "UploadFileParameters",
  "GenerateContentRequest",
  "Schema",
  "Tool",
  "ToolConfig",
  "FunctionDeclaration",
  "ThinkingConfig",
  "SafetySetting",
];
for (const k of keyTypes) {
  if (!emitted.has(k)) continue;
  const t = allTypes.find((x) => x.name === k);
  const shown = t.fields.slice(0, 8).join("、");
  lines.push(`- \`${k}\`（${t.fields.length} 字段${t.unresolved ? "，含未解析成员" : ""}）：${shown}${t.fields.length > 8 ? " …" : ""}`);
}
lines.push("");
writeFileSync(join(apiDir, "SUMMARY.md"), lines.join("\n"));

// ---------------------------------------------------------------------------
// 10. 上游自带 API 文档
// ---------------------------------------------------------------------------
const upstreamCandidates = ["api.md", join("docs", "api.md"), join("docs", "README.md")];
let copiedUpstream = false;
for (const rel of upstreamCandidates) {
  const p = join(sdkDir, rel);
  if (existsSync(p)) {
    copyFileSync(p, join(apiDir, "upstream-api.md"));
    copiedUpstream = true;
    break;
  }
}
if (!copiedUpstream) {
  writeFileSync(
    join(apiDir, "upstream-api.md"),
    [
      "# 上游 API 文档：无单独 api.md",
      "",
      `\`${pkg.name}@${version}\` 的 npm tarball 没有自带 \`api.md\` / \`docs/api.md\`（只有 \`README.md\` 使用说明，已随 SDK 快照保留在 \`sdk/…/README.md\`）。`,
      "",
      "因此本文件由 `scripts/extract-api.mjs` 生成，仅作占位与指引：",
      "",
      "- 官方 REST 参考（本次快照已抓取）：`docs/api-reference__generate-content__generate-content.md`、`docs/api-reference__all-methods.md`",
      "- 全部类型与 JSDoc：`sdk/" + basename(sdkDir) + "/dist/genai.d.ts`",
      "- 机器可读清单：`api/manifest.json`",
      "",
    ].join("\n"),
  );
}

console.log(
  `manifest: ${coverage.resources} resources, ${coverage.methods} methods (${withHttp} with HTTP), ` +
    `types[] ${coverage.params_types} *Parameters + other_types[] ${coverage.other_types} = ${coverage.types_total} (${unresolved} unresolved)`,
);
