#!/usr/bin/env node
/**
 * 从裁剪后的 @anthropic-ai/sdk 目录静态生成 API 清单。
 *
 * 输入：sdk/<pkg>@<version>/（只含 .d.ts、resources/**\/*.js、package.json、VERSION、SOURCE）
 * 输出：api/manifest.json、api/SUMMARY.md，并在存在时复制 api.md -> api/upstream-api.md
 *
 * 原则：
 *   - 纯静态文本解析，不 require/import 包内任何代码，不联网，不依赖 typescript 编译器。
 *   - 结构信息全部来自 .d.ts（class / interface / type alias）；
 *     HTTP 动词与路径来自 resources/**\/*.js 里的 `this._client.<verb>('...')` /
 *     `this._client.getAPIList('...')`，路径里的 `${x}` 归一化成 `{x}`。
 *   - 解析不到的东西保留名称并标注（http: null / unresolved），不省略。
 *
 * 用法：node scripts/extract-api.mjs <sdk-dir> [api-dir]
 */

import { readdirSync, readFileSync, writeFileSync, mkdirSync, copyFileSync, existsSync } from 'node:fs';
import { join, dirname, relative, sep, posix } from 'node:path';

const sdkDir = process.argv[2];
const apiDir = process.argv[3] || join(dirname(process.argv[2]), '..', 'api');
if (!sdkDir) {
  console.error('usage: extract-api.mjs <sdk-dir> [api-dir]');
  process.exit(2);
}

/* ------------------------------------------------------------------ 基础工具 */

/** 去掉 // 与 块注释，保留字符串/模板字面量内容。d.ts 里的 JSDoc 含 { } ` 会干扰结构匹配。 */
function stripComments(src) {
  let out = '';
  let i = 0;
  const n = src.length;
  while (i < n) {
    const c = src[i];
    const d = src[i + 1];
    if (c === '/' && d === '/') {
      while (i < n && src[i] !== '\n') i++;
      continue;
    }
    if (c === '/' && d === '*') {
      i += 2;
      while (i < n && !(src[i] === '*' && src[i + 1] === '/')) {
        if (src[i] === '\n') out += '\n';
        i++;
      }
      i += 2;
      continue;
    }
    if (c === '"' || c === "'" || c === '`') {
      const quote = c;
      out += c;
      i++;
      while (i < n) {
        if (src[i] === '\\') {
          out += src[i] + (src[i + 1] ?? '');
          i += 2;
          continue;
        }
        out += src[i];
        if (src[i] === quote) {
          i++;
          break;
        }
        i++;
      }
      continue;
    }
    out += c;
    i++;
  }
  return out;
}

/** src[openIdx] 是 {/([/<，返回配对闭合字符的下标。 */
function matchClose(src, openIdx) {
  const open = src[openIdx];
  const close = open === '{' ? '}' : open === '(' ? ')' : open === '[' ? ']' : '>';
  let depth = 0;
  for (let i = openIdx; i < src.length; i++) {
    const c = src[i];
    if (c === '"' || c === "'" || c === '`') {
      const q = c;
      i++;
      while (i < src.length && src[i] !== q) {
        if (src[i] === '\\') i++;
        i++;
      }
      continue;
    }
    if (c === open) depth++;
    else if (c === close) {
      depth--;
      if (depth === 0) return i;
    }
  }
  return -1;
}

/**
 * 把 class / interface 的 body 切成顶层成员（以 depth0 的 ';' 或 ',' 或换行结束）。
 * 只跟踪 () [] {} 与字符串；`<`/`>` 不参与（`=>` 使其不可靠，且成员边界不会出现在泛型里）。
 */
/**
 * 按顶层分隔符切分（跟踪 () [] {} 深度 + 泛型 <> 深度）。
 * - 泛型里的逗号不能切断成员：`PagePromise<A, B>` / `Record<string, number>`。
 * - `=>` 里的 `>` 必须当箭头处理而不是泛型闭合，否则深度会漂移。
 */
function splitTopLevel(str, separators) {
  const parts = [];
  let buf = '';
  let depth = 0;
  let angle = 0;
  let prevSignificant = '';
  for (let i = 0; i < str.length; i++) {
    const c = str[i];
    if (c === '"' || c === "'" || c === '`') {
      const q = c;
      let j = i + 1;
      while (j < str.length && str[j] !== q) {
        if (str[j] === '\\') j++;
        j++;
      }
      buf += str.slice(i, j + 1);
      i = j;
      prevSignificant = q;
      continue;
    }
    if (c === '=' && str[i + 1] === '>') {
      buf += '=>';
      i++;
      prevSignificant = '>';
      continue;
    }
    if (c === '(' || c === '[' || c === '{') depth++;
    else if (c === ')' || c === ']' || c === '}') depth--;
    else if (c === '<' && /[\w$)\]>]/.test(prevSignificant)) angle++;
    else if (c === '>' && angle > 0) angle--;
    if (separators.includes(c) && depth === 0 && angle === 0) {
      const t = buf.trim();
      if (t) parts.push(t);
      buf = '';
      continue;
    }
    buf += c;
    if (!/\s/.test(c)) prevSignificant = c;
  }
  const t = buf.trim();
  if (t) parts.push(t);
  return parts;
}

function splitMembers(body) {
  return splitTopLevel(body, [';', ',']);
}

function splitTop(str, sep) {
  return splitTopLevel(str, [sep]);
}

/** 'A | B<C> | null | undefined' -> 'A | B<C>'；并取出所有标识符。 */
function cleanType(t) {
  return t
    .replace(/\s+/g, ' ')
    .replace(/^\|\s*/, '')
    .replace(/\|\s*(undefined|null)\s*(?=\||$)/g, '')
    .replace(/\s*\|\s*$/, '')
    .trim();
}

const identRe = /\b([A-Za-z_$][\w$]*)\b/g;
function idents(t) {
  return [...new Set([...t.matchAll(identRe)].map((m) => m[1]))];
}

function walk(dir, base, acc = []) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) {
      if (entry.name === 'node_modules') continue;
      walk(full, base, acc);
    } else if (entry.isFile()) {
      acc.push({ full, rel: relative(base, full).split(sep).join(posix.sep) });
    }
  }
  return acc;
}

/* ------------------------------------------------------------ .d.ts 静态解析 */

const allFiles = walk(sdkDir, sdkDir);
const dtsFiles = allFiles.filter((f) => f.rel.endsWith('.d.ts'));
const jsFiles = allFiles.filter((f) => f.rel.endsWith('.js') && f.rel.startsWith('resources/'));

/** name -> [{ file, kind, parent, fields, memberMethods, raw }] */
const typeIndex = new Map();
/** name -> [{ file, extends, methods: [{name, params, returns, isProperty}] }] */
const classIndex = new Map();

function addType(name, entry) {
  if (!typeIndex.has(name)) typeIndex.set(name, []);
  typeIndex.get(name).push(entry);
}

const srcCache = new Map();
function srcOf(rel) {
  if (!srcCache.has(rel)) srcCache.set(rel, stripComments(readFileSync(join(sdkDir, rel), 'utf8')));
  return srcCache.get(rel);
}

for (const { rel } of dtsFiles) {
  const src = srcOf(rel);

  // class 声明
  const classRe = /\bexport declare class\s+([A-Za-z_$][\w$]*)[^{]*\{/g;
  for (const m of src.matchAll(classRe)) {
    const name = m[1];
    const open = m.index + m[0].length - 1;
    const close = matchClose(src, open);
    if (close < 0) continue;
    const body = src.slice(open + 1, close);
    const head = m[0];
    const ext = /extends\s+([A-Za-z_$][\w$]*)/.exec(head);
    const methods = [];
    for (const member of splitMembers(body)) {
      if (/^constructor\b/.test(member)) continue;
      const sig = /^(?:async\s+)?([A-Za-z_$][\w$]*)\s*(?:<[^(]*>)?\s*\(/.exec(member);
      if (!sig) continue; // 属性（子 resource 引用）在 graph 步骤里单独处理
      const parenOpen = member.indexOf('(', sig[0].length - 1);
      const parenClose = matchClose(member, parenOpen);
      if (parenClose < 0) continue;
      const argsRaw = member.slice(parenOpen + 1, parenClose);
      const after = member.slice(parenClose + 1);
      const retMatch = /^:\s*([\s\S]+)$/.exec(after.trim());
      const args = splitTop(argsRaw, ',').map((a) => {
        const mm = /^([A-Za-z_$][\w$]*)\s*\??\s*:\s*([\s\S]+)$/.exec(a);
        return mm ? { name: mm[1], type: cleanType(mm[2]) } : null;
      }).filter(Boolean);
      methods.push({
        name: sig[1],
        args,
        returns: retMatch ? retMatch[1].replace(/\s+/g, ' ').trim() : 'void',
      });
    }
    if (!classIndex.has(name)) classIndex.set(name, []);
    classIndex.get(name).push({ file: rel, extends: ext ? ext[1] : null, methods });
  }

  // interface（含 extends）
  const ifaceRe = /\bexport interface\s+([A-Za-z_$][\w$]*)[^{]*\{/g;
  for (const m of src.matchAll(ifaceRe)) {
    const name = m[1];
    const open = m.index + m[0].length - 1;
    const close = matchClose(src, open);
    if (close < 0) continue;
    const head = m[0].replace(/\{[\s\S]*$/, '');
    const extMatch = /extends\s+([^{]+)$/.exec(head);
    const parents = extMatch ? splitTop(extMatch[1], ',') : [];
    const body = src.slice(open + 1, close);
    const fields = [];
    for (const member of splitMembers(body)) {
      const f = /^(?:readonly\s+)?([A-Za-z_$][\w$]*)\s*\??\s*:\s*([\s\S]+)$/.exec(member);
      if (f) {
        fields.push(f[1]);
        continue;
      }
      // ['key': string]: unknown 之类的索引签名 / 方法签名忽略
    }
    addType(name, { file: rel, kind: 'interface', parents, fields });
  }

  // type alias：`export type X = A | B;`
  const aliasRe = /\bexport type\s+([A-Za-z_$][\w$]*)\s*=\s*([^;]+);/g;
  for (const m of src.matchAll(aliasRe)) {
    addType(m[1], {
      file: rel,
      kind: 'alias',
      aliasOf: splitTop(cleanType(m[2]), '|'),
      parents: [],
      fields: [],
    });
  }
}

/** 同名声明在多文件里重复（GA 与 beta 的 namespace 各有一份），优先同文件，再全局唯一。 */
function resolveLocal(name, file) {
  const list = typeIndex.get(name);
  if (!list) return null;
  return list.find((t) => t.file === file) || (list.length === 1 ? list[0] : null);
}

/** 递归取字段：自身 + extends 的字段（同名去重，父类字段在前）。 */
function fieldsOf(name, file, seen = new Set()) {
  const key = `${file}::${name}`;
  if (seen.has(key)) return [];
  seen.add(key);
  const t = resolveLocal(name, file);
  if (!t) return [];
  const out = [];
  for (const p of t.parents || []) {
    const pn = p.replace(/<[\s\S]*$/, '').trim();
    for (const f of fieldsOf(pn, file, seen)) if (!out.includes(f)) out.push(f);
  }
  for (const f of t.fields || []) if (!out.includes(f)) out.push(f);
  if (t.kind === 'alias') {
    for (const member of t.aliasOf || []) {
      const mn = member.replace(/<[\s\S]*$/, '').trim();
      if (!/^[A-Za-z_$][\w$]*$/.test(mn)) continue;
      for (const f of fieldsOf(mn, file, seen)) if (!out.includes(f)) out.push(f);
    }
  }
  return out;
}

function existsType(name, file) {
  return !!resolveLocal(name, file);
}

/* ------------------------------------------------------- resource 层级（BFS） */

// resources/index.d.ts: 导出名 -> 文件
const exportMap = new Map();
if (existsSync(join(sdkDir, 'resources/index.d.ts'))) {
  const src = srcOf('resources/index.d.ts');
  for (const m of src.matchAll(/export\s*\{([^}]*)\}\s*from\s*"([^"]+)"/g)) {
    const from = posix.normalize(posix.join('resources', m[2].replace(/^\.\//, ''))).replace(/\.js$/, '.d.ts');
    for (const raw of m[1].split(',')) {
      const nm = raw.trim().replace(/^type\s+/, '').trim();
      if (nm) exportMap.set(nm, from);
    }
  }
}

// .js 里的子 resource：this.<prop> = new <Alias>.<Class>(this._client)
const childrenOf = new Map(); // js rel -> [{prop, file}]
const httpOf = new Map(); // js rel -> Map(methodName -> {verb, path})
for (const { rel, full } of jsFiles) {
  const src = stripComments(readFileSync(full, 'utf8'));
  const aliases = new Map();
  for (const m of src.matchAll(/\b(?:var|const|let)\s+([A-Za-z_$][\w$]*)\s*=\s*require\("(\.[^"]+)"\)/g)) {
    aliases.set(m[1], posix.normalize(posix.join(posix.dirname(rel), m[2])).replace(/\.js$/, '.d.ts'));
  }

  const kids = [];
  for (const m of src.matchAll(/this\.([A-Za-z_$][\w$]*)\s*=\s*new\s+([A-Za-z_$][\w$]*)\.([A-Za-z_$][\w$]*)\(/g)) {
    const file = aliases.get(m[2]);
    if (file) kids.push({ prop: m[1], file, cls: m[3] });
  }
  childrenOf.set(rel, kids);

  // HTTP：把 `this._client.<verb>(path)` 归到它前面最近的那个 class 方法上。
  // 注意缩进不固定：GA 资源编译在 `var X = (() => { class X ... })()` 里（8 空格），
  // beta 资源有的直接 `class X extends APIResource {`（4 空格），所以不能写死缩进，
  // 用“最近的前置方法声明”配对（同一方法内多次调用取第一次字面量路径）。
  //
  // 三层信息，逐级降级，全部来自静态文本：
  //   1. `this._client.<verb>('<path>')` / `getAPIList('<path>')` -> 动词 + 路径
  //   2. 路径是运行时变量时（如 batches.results 用 `batch.results_url`），
  //      退而取同一方法里 tracing 元数据的 `route: '/v1/...'` 作为路径
  //   3. 都没有 -> http: null（不猜）
  const lines = src.split('\n');
  const http = new Map();
  const reserved = new Set(['if', 'for', 'while', 'switch', 'catch', 'return', 'function', 'do', 'else', 'constructor']);
  const decls = []; // { line, name }
  lines.forEach((line, idx) => {
    const m = /^\s{2,20}(?:async\s+)?([A-Za-z_$][\w$]*)\s*\(([^()]*)\)\s*\{\s*$/.exec(line);
    if (m && !reserved.has(m[1])) decls.push({ line: idx, name: m[1] });
  });
  const found = new Map(); // method -> {verb, path, route}
  const norm = (p) => p.replace(/\$\{([^}]+)\}/g, '{$1}');
  let d = 0;
  lines.forEach((line, idx) => {
    while (d + 1 < decls.length && decls[d + 1].line < idx) d++;
    if (!decls.length || decls[0].line > idx) return;
    const rec = found.get(decls[d].name) || {};
    const direct = /this\._client\.(get|post|put|patch|delete)\(\s*(?:\(0,\s*[\w$]+\.path\)\s*)?[`'"]([^`'"]+)[`'"]/.exec(line);
    const listCall = direct ? null : /this\._client\.getAPIList\(\s*(?:\(0,\s*[\w$]+\.path\)\s*)?[`'"]([^`'"]+)[`'"]/.exec(line);
    if (direct || listCall) {
      if (!rec.path) {
        rec.verb = direct ? direct[1].toUpperCase() : 'GET';
        rec.path = norm(direct ? direct[2] : listCall[1]);
      }
    } else if (!rec.verb) {
      // 跨行的 `this._client\n  .get(...)`（路径是运行时变量，只能拿到动词）
      const two = (line + ' ' + (lines[idx + 1] || '')).replace(/\s+/g, '');
      const v = /this\._client\.?(get|post|put|patch|delete)\(/.exec(two);
      if (v) rec.verb = v[1].toUpperCase();
    }
    if (!rec.route) {
      const r = /\broute:\s*'([^']+)'/.exec(line);
      if (r) rec.route = r[1];
    }
    found.set(decls[d].name, rec);
  });
  for (const [name, rec] of found) {
    const path = rec.path || rec.route;
    if (path && rec.verb) http.set(name, { verb: rec.verb, path });
  }
  httpOf.set(rel, http);
}

const dtsForJs = (jsRel) => jsRel.replace(/\.js$/, '.d.ts');

// 根 resource 来自 client.d.ts 里 class Anthropic 的 `messages: API.Messages;`
const roots = [];
{
  const rel = 'client.d.ts';
  if (existsSync(join(sdkDir, rel))) {
    const src = srcOf(rel);
    const m = /\bexport declare class Anthropic\b[^{]*\{/.exec(src);
    if (m) {
      const open = m.index + m[0].length - 1;
      const close = matchClose(src, open);
      const body = src.slice(open + 1, close);
      for (const mm of body.matchAll(/^\s{4}([A-Za-z_$][\w$]*)\s*:\s*API\.([A-Za-z_$][\w$]*)\s*;/gm)) {
        const file = exportMap.get(mm[2]);
        if (file) roots.push({ path: mm[1], file, cls: mm[2] });
      }
    }
  }
}

const resourceOf = new Map(); // d.ts rel -> resource path
const queue = [...roots];
while (queue.length) {
  const cur = queue.shift();
  if (resourceOf.has(cur.file)) continue;
  resourceOf.set(cur.file, cur.path);
  const jsRel = cur.file.replace(/\.d\.ts$/, '.js');
  for (const kid of childrenOf.get(jsRel) || []) {
    if (!resourceOf.has(kid.file)) queue.push({ path: `${cur.path}.${kid.prop}`, file: kid.file, cls: kid.cls });
  }
}

// 兜底：未被 BFS 覆盖的 APIResource 类，用文件路径推导（resources/beta/x/y.d.ts -> beta.x.y）
// 只认 resources/ 下、直接继承 APIResource 的类；core/ lib/ 里的 Page/APIPromise/ToolRunner
// 不是 API resource，必须排除。
const classFiles = new Map();
const resourceClasses = new Set();
for (const [name, entries] of classIndex) {
  for (const e of entries) {
    if (e.extends === 'APIResource' && e.file.startsWith('resources/')) resourceClasses.add(name);
    if (!e.extends) continue;
    if (!classFiles.has(e.file)) classFiles.set(e.file, []);
    classFiles.get(e.file).push(name);
  }
}
function derivedPath(rel) {
  const parts = rel
    .replace(/^resources\//, '')
    .replace(/\.d\.ts$/, '')
    .split('/')
    .filter((p) => p !== 'index');
  const camel = (s) => s.replace(/-([a-z0-9])/g, (_, c) => c.toUpperCase());
  const cleaned = [];
  for (const p of parts) {
    const c = camel(p);
    if (cleaned[cleaned.length - 1] !== c) cleaned.push(c);
  }
  return cleaned.join('.');
}

/* -------------------------------------------------------------- method 归一化 */

const methods = [];
const resourcePaths = new Set();

for (const [file, resource] of [...resourceOf.entries()].sort((a, b) => a[1].localeCompare(b[1]))) {
  const jsRel = file.replace(/\.d\.ts$/, '.js');
  const httpMap = httpOf.get(jsRel) || new Map();
  const classes = classFiles.get(file) || [];
  for (const cls of classes) {
    const entries = classIndex.get(cls) || [];
    const entry = entries.find((e) => e.file === file) || entries[0];
    if (!entry) continue;
    resourcePaths.add(resource);
    // 同名方法的重载（create 有 3 个签名）合并成一条
    const byName = new Map();
    for (const m of entry.methods) {
      if (m.name.startsWith("_")) continue;
      if (!byName.has(m.name)) byName.set(m.name, []);
      byName.get(m.name).push(m);
    }
    for (const [name, overloads] of byName) {
      const types = overloads.map(
        (o) => o.args.find((a) => /\b[A-Za-z0-9_]*Params(?:Base|Streaming|NonStreaming)?$/.test(a.type))?.type || null,
      );
      let params = null;
      if (types.some(Boolean)) {
        const distinct = [...new Set(types.filter(Boolean))];
        if (distinct.length === 1) {
          params = distinct[0];
        } else {
          // MessageCreateParamsNonStreaming / MessageCreateParamsStreaming / MapMessageCreateParamsBase
          // -> 公共前缀 MessageCreateParams（上游通常正好有同名 union alias）
          let prefix = distinct[0];
          for (const t of distinct.slice(1)) {
            let i = 0;
            while (i < prefix.length && i < t.length && prefix[i] === t[i]) i++;
            prefix = prefix.slice(0, i);
          }
          const cand = distinct.find((t) => t === prefix);
          params = cand || (existsType(prefix, file) ? prefix : distinct[distinct.length - 1]);
        }
      }
      const ret = overloads[overloads.length - 1].returns;
      const http = httpMap.get(name) || null;
      methods.push({
        resource,
        method: name,
        params,
        returns: ret || 'unresolved',
        http: http ? `${http.verb} ${http.path}` : null,
        _file: file,
        _overloads: overloads.length,
      });
    }
  }
}

// 兜底：未被 BFS 覆盖的 APIResource 类
for (const [file, classes] of [...classFiles.entries()].sort((a, b) => a[0].localeCompare(b[0]))) {
  if (resourceOf.has(file)) continue;
  const resourceCls = classes.filter((c) => resourceClasses.has(c));
  if (resourceCls.length === 0) continue;
  const path = derivedPath(file);
  if (!path) continue;
  const jsRel = file.replace(/\.d\.ts$/, '.js');
  const httpMap = httpOf.get(jsRel) || new Map();
  for (const cls of resourceCls) {
    const entries = classIndex.get(cls) || [];
    const entry = entries.find((e) => e.file === file) || entries[0];
    if (!entry || entry.methods.length === 0) continue;
    resourcePaths.add(path);
    const byName = new Map();
    for (const m of entry.methods) {
      if (m.name.startsWith("_")) continue;
      if (!byName.has(m.name)) byName.set(m.name, []);
      byName.get(m.name).push(m);
    }
    for (const [name, overloads] of byName) {
      const paramType = overloads
        .map((o) => o.args.find((a) => /Params/.test(a.type))?.type || null)
        .filter(Boolean);
      const http = httpMap.get(name) || null;
      methods.push({
        resource: path,
        method: name,
        params: paramType.length ? paramType[paramType.length - 1] : null,
        returns: overloads[overloads.length - 1].returns,
        http: http ? `${http.verb} ${http.path}` : null,
        _file: file,
        _overloads: overloads.length,
      });
    }
  }
}

methods.sort((a, b) => a.resource.localeCompare(b.resource) || 0);

/* --------------------------------------------------------------- types 清单 */

const types = [];
for (const [name, entries] of [...typeIndex.entries()].sort((a, b) => a[0].localeCompare(b[0]))) {
  for (const e of entries.sort((a, b) => a.file.localeCompare(b.file))) {
    // 只收“请求体类型”：XxxParams / XxxParamsBase / XxxParamsStreaming / XxxParamsNonStreaming
    if (!/[A-Za-z0-9_]Params(?:Base|Streaming|NonStreaming)?$/.test(name)) continue;
    types.push({
      name,
      file: e.file,
      fields: fieldsOf(name, e.file),
    });
  }
}
types.sort((a, b) => a.file.localeCompare(b.file) || a.name.localeCompare(b.name));

const fieldCount = new Map();
for (const t of types) if (!fieldCount.has(t.name)) fieldCount.set(t.name, t.fields.length);

// 同名 Params 可能同时存在于 GA / beta 文件，字段数要按文件区分
const fieldCounts = new Map();
for (const t of types) {
  if (!fieldCounts.has(t.name)) fieldCounts.set(t.name, []);
  fieldCounts.get(t.name).push({ file: t.file, count: t.fields.length });
}

/* ------------------------------------------------------------------- manifest */

const version = (() => {
  try {
    return readFileSync(join(sdkDir, 'VERSION'), 'utf8').trim();
  } catch {
    try {
      return JSON.parse(readFileSync(join(sdkDir, 'package.json'), 'utf8')).version;
    } catch {
      return 'unknown';
    }
  }
})();

const manifest = {
  provider: 'anthropic',
  package: '@anthropic-ai/sdk',
  version,
  generated_at: new Date().toISOString().replace(/\.\d{3}Z$/, 'Z'),
  source: 'https://registry.npmjs.org/@anthropic-ai/sdk',
  coverage: {
    // resources 与 methods[].resource 的唯一值数严格一致（校验时会断言）
    resources: new Set(methods.map((m) => m.resource)).size,
    methods: methods.length,
    params_types: types.length,
  },
  methods: methods.map(({ resource, method, params, returns, http }) => ({
    resource,
    method,
    params,
    returns,
    http,
  })),
  types,
};

mkdirSync(apiDir, { recursive: true });
writeFileSync(join(apiDir, 'manifest.json'), JSON.stringify(manifest, null, 2) + '\n');

const upstream = join(sdkDir, 'api.md');
if (existsSync(upstream)) copyFileSync(upstream, join(apiDir, 'upstream-api.md'));

/* ------------------------------------------------------------------ SUMMARY.md */

const byResource = new Map();
manifest.methods.forEach((m, i) => {
  if (!byResource.has(m.resource)) byResource.set(m.resource, []);
  byResource.get(m.resource).push({ ...m, file: methods[i]._file });
});

/** 参数类型 -> 字段数；同名声明可能存在于 GA 与 beta 两个文件，优先同文件。 */
const paramCount = (name, file) => {
  if (!name) return undefined;
  const list = fieldCounts.get(name);
  if (!list || !list.length) return undefined;
  const exact = list.find((x) => x.file === file);
  return (exact || list[0]).count;
};
const escaped = (s) => String(s ?? '—').replace(/\|/g, '\\|');
const lines = [
  '# Anthropic API 清单（来自官方 `@anthropic-ai/sdk`）',
  '',
  `- 包：\`${manifest.package}@${manifest.version}\``,
  // 刻意不写生成时间：SUMMARY.md 是给人看 diff 的，时间戳会把“接口有变化”淹没在噪声里；
  // 生成时间在 manifest.json / docs/meta.json 里。
  `- 来源：${manifest.source}`,
  `- 解析方式：静态解析裁剪后的 \`.d.ts\`（类型）与 \`resources/**/*.js\`（HTTP 动词 + 路径），未执行上游代码`,
  `- 覆盖：**${manifest.coverage.resources}** 个 resource、**${manifest.coverage.methods}** 个方法、**${manifest.coverage.params_types}** 条 \`*Params\` 类型声明（字段数含继承字段）`,
  '',
  '说明：',
  '',
  '- `HTTP` 为静态可发现的端点；`—` 表示该方法没有直接的 `_client.<verb>()` 调用（例如 SDK 便捷方法 `stream` / `parse` / `toolRunner`，内部转调其他方法），或路径是运行时拼接的变量。',
  '- `参数` 为该方法请求体的 TypeScript 类型名；重载方法（如 `messages.create` 的 streaming / non-streaming 签名）合并为一条：优先取重载类型名的公共前缀（GA 下正好是上游声明的 union alias `MessageCreateParams`），beta 下没有对应 alias 时退回最后一个（最宽）重载 `MessageCreateParamsBase`。',
  '- `参数字段数` 统计该类型声明的顶层字段（含 `extends` 继承来的字段，如 `PageParams` 的 `limit`/`after_id`；也包含 `workspace_id`/`betas` 这类 header 字段，因为它们在 SDK 里就是同一个入参对象的字段）。',
  '- beta 命名空间下的同名方法（如 `beta.messages.create` 与 `messages.create`）**两条都保留**：resource 不同、请求头/可选字段不同，去重会丢信息。',
  '- `params_types` 计的是**声明条数**（同一名字在 GA 与 beta 文件里各声明一次就会算两次），不是唯一名字数。',
  '',
];

for (const [resource, list] of byResource) {
  lines.push(`## \`${resource}\``, '');
  const withHttp = list.filter((m) => m.http).length;
  lines.push(`${list.length} 个方法，其中 ${withHttp} 个可直接映射到 HTTP 端点。`, '');
  lines.push('| 方法 | 参数类型 | HTTP | 参数字段数 |');
  lines.push('| --- | --- | --- | --- |');
  for (const m of list) {
    const n = paramCount(m.params, m.file);
    lines.push(
      `| \`${m.method}\` | ${m.params ? '`' + m.params + '`' : '—'} | ${m.http ? '`' + m.http + '`' : '—'} | ${
        n === undefined ? '—' : n
      } |`,
    );
  }
  lines.push('');
}

const httpCount = manifest.methods.filter((m) => m.http).length;
const uniqueParamNames = new Set(manifest.types.map((t) => t.name)).size;
lines.push(
  '## 汇总',
  '',
  `- resource：${manifest.coverage.resources}`,
  `- 方法：${manifest.coverage.methods}（其中 ${httpCount} 个带静态可解析的 HTTP 端点）`,
  `- \`*Params\` 类型声明：${manifest.coverage.params_types}（唯一名字 ${uniqueParamNames} 个）`,
  `- 所有方法都声明了参数类型的比例：${
    manifest.methods.filter((m) => m.params).length
  }/${manifest.coverage.methods}`,
  '',
);

writeFileSync(join(apiDir, 'SUMMARY.md'), lines.join('\n'));

console.error(
  `==> api: ${manifest.coverage.resources} resources, ${manifest.coverage.methods} methods, ${manifest.coverage.params_types} params types`,
);
