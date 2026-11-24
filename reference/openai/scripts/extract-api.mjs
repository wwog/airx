#!/usr/bin/env node
// 静态解析裁剪后的 openai npm 包，生成 api/manifest.json 与 api/SUMMARY.md。
//
// 安全约定：本脚本只把 .d.ts / .js / .md 当**文本数据**读取，
// 绝不 import / require / eval 上游代码，也不执行其安装脚本。
//
// 用法:
//   node extract-api.mjs --sdk sdk/openai@7.63.0 --out api [--generated-at 2026-01-01T00:00:00Z]
//
// 解析策略（无 TypeScript 依赖，纯正则 + 括号扫描）:
//   * resource / method / params / returns 来自 resources/**/*.d.ts
//     （`export declare class X extends APIResource` 内 4 空格缩进的方法声明，支持跨行签名）
//   * HTTP verb + path 来自同名 resources/**/*.js 中 `this._client.<verb>('...')` 的首个字面量；
//     模板串里的 `${responseID}` 归一为 `{responseID}`
//   * *Params 请求体接口的顶层字段名按「接口声明缩进 + 4」列提取

import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { join, relative, sep } from 'node:path';

// --------------------------------------------------------------- CLI 参数

function parseArgs(argv) {
  const out = { generatedAt: null, sdk: null, out: null };
  for (let i = 2; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === '--sdk') out.sdk = argv[++i];
    else if (arg === '--out') out.out = argv[++i];
    else if (arg === '--generated-at') out.generatedAt = argv[++i];
    else if (arg === '-h' || arg === '--help') {
      process.stdout.write('usage: extract-api.mjs --sdk <dir> --out <dir> [--generated-at <iso>]\n');
      process.exit(0);
    } else {
      process.stderr.write(`unknown argument: ${arg}\n`);
      process.exit(2);
    }
  }
  if (!out.sdk || !out.out) {
    process.stderr.write('usage: extract-api.mjs --sdk <dir> --out <dir> [--generated-at <iso>]\n');
    process.exit(2);
  }
  if (!existsSync(join(out.sdk, 'package.json'))) {
    process.stderr.write(`not an extracted SDK dir (no package.json): ${out.sdk}\n`);
    process.exit(2);
  }
  return out;
}

// --------------------------------------------------------------- 文本工具

// 单遍扫描，把注释（以及可选的字符串内容）替换成等长空格，保留换行与列位置。
// 必须字符串感知：JS/TS 里存在 'https://...' 这类内含 `//` 的字符串，
// 先切注释再切字符串会把后续代码整段当成注释吞掉。
function scanBlank(text, { blankStrings = true } = {}) {
  const out = text.split('');
  const erase = (from, to) => {
    for (let k = from; k < to && k < text.length; k += 1) {
      if (out[k] !== '\n') out[k] = ' ';
    }
  };
  let i = 0;
  while (i < text.length) {
    const c = text[i];
    const n = text[i + 1];
    if (c === '/' && n === '/') {
      const start = i;
      while (i < text.length && text[i] !== '\n') i += 1;
      erase(start, i);
      continue;
    }
    if (c === '/' && n === '*') {
      const start = i;
      i += 2;
      while (i < text.length && !(text[i] === '*' && text[i + 1] === '/')) i += 1;
      i = Math.min(i + 2, text.length);
      erase(start, i);
      continue;
    }
    if (c === "'" || c === '"' || c === '`') {
      const start = i;
      const quote = c;
      i += 1;
      while (i < text.length && text[i] !== quote) i += text[i] === '\\' ? 2 : 1;
      i = Math.min(i + 1, text.length);
      if (blankStrings) erase(start + 1, i - 1);
      continue;
    }
    i += 1;
  }
  return out.join('');
}

// 从 openIndex 的开括号找到配对闭括号（跳过字符串与注释）。
function matchBracket(text, openIndex) {
  const expected = { '{': '}', '(': ')', '[': ']' }[text[openIndex]];
  if (!expected) return -1;
  let depth = 0;
  let i = openIndex;
  while (i < text.length) {
    const c = text[i];
    if (c === '/' && text[i + 1] === '/') {
      while (i < text.length && text[i] !== '\n') i += 1;
      continue;
    }
    if (c === '/' && text[i + 1] === '*') {
      i += 2;
      while (i < text.length && !(text[i] === '*' && text[i + 1] === '/')) i += 1;
      i += 2;
      continue;
    }
    if (c === "'" || c === '"' || c === '`') {
      const quote = c;
      i += 1;
      while (i < text.length && text[i] !== quote) {
        i += text[i] === '\\' ? 2 : 1;
      }
      i += 1;
      continue;
    }
    if (c === '(' || c === '{' || c === '[') depth += 1;
    else if (c === ')' || c === '}' || c === ']') {
      depth -= 1;
      if (depth === 0) return i;
    }
    i += 1;
  }
  return -1;
}

// 按顶层分隔符切分（忽略括号/尖括号/字符串内部）。
function splitTopLevel(text, separator) {
  const parts = [];
  let depth = 0;
  let start = 0;
  let i = 0;
  while (i < text.length) {
    const c = text[i];
    if (c === '/' && text[i + 1] === '/') {
      while (i < text.length && text[i] !== '\n') i += 1;
      continue;
    }
    if (c === '/' && text[i + 1] === '*') {
      i += 2;
      while (i < text.length && !(text[i] === '*' && text[i + 1] === '/')) i += 1;
      i += 2;
      continue;
    }
    if (c === "'" || c === '"' || c === '`') {
      const quote = c;
      i += 1;
      while (i < text.length && text[i] !== quote) {
        i += text[i] === '\\' ? 2 : 1;
      }
      i += 1;
      continue;
    }
    if (c === '(' || c === '{' || c === '[' || c === '<') depth += 1;
    else if (c === ')' || c === '}' || c === ']' || c === '>') depth -= 1;
    else if (c === separator && depth === 0) {
      parts.push(text.slice(start, i));
      start = i + 1;
    }
    i += 1;
  }
  parts.push(text.slice(start));
  return parts;
}

const squash = (text) => text.replace(/\s+/g, ' ').trim();

// `ChatCompletionCreateParams & {...}` -> `ChatCompletionCreateParams`
// `(InputItemListParams & {...}) | null` -> `InputItemListParams`
function leadingTypeName(typeText) {
  let text = squash(typeText);
  while (text.startsWith('(')) text = text.slice(1).trim();
  const match = /^[A-Za-z_$][\w$.]*/.exec(text);
  return match ? match[0] : text;
}

function lineIndent(text, index) {
  const lineStart = text.lastIndexOf('\n', index - 1) + 1;
  let i = lineStart;
  while (i < text.length && (text[i] === ' ' || text[i] === '\t')) i += 1;
  return i - lineStart;
}

function lineOffsets(body) {
  const offsets = [];
  let offset = 0;
  for (const line of body.split('\n')) {
    offsets.push(offset);
    offset += line.length + 1;
  }
  return offsets;
}

// --------------------------------------------------------------- 文件遍历

function walk(dir, predicate, acc = []) {
  for (const entry of readdirSync(dir, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) walk(full, predicate, acc);
    else if (predicate(full)) acc.push(full);
  }
  return acc;
}

const toPosix = (p) => p.split(sep).join('/');

// resources/chat/completions/completions.d.ts -> chat.completions
// resources/batches.d.ts                       -> batches
function resourceNameFromPath(relPath) {
  const segments = relPath.replace(/\.d\.ts$/, '').split('/');
  if (segments[0] === 'resources') segments.shift();
  if (segments.length >= 2) {
    const fileBase = segments[segments.length - 1];
    if (segments[segments.length - 2] === fileBase) segments.pop();
  }
  return segments.join('.');
}

// ------------------------------------------------------------- 接口 / 类型

// 返回该文件里所有 interface 的 { name, fields, extendsText }。
function extractInterfaces(raw) {
  const commentFree = scanBlank(raw, { blankStrings: false });
  const structural = scanBlank(raw);
  const found = [];
  const re = /(?:^|\n)([ \t]*)(?:export\s+)?interface\s+([A-Za-z_$][\w$]*)([^;{]*)\{/g;
  let match;
  while ((match = re.exec(structural)) !== null) {
    const name = match[2];
    const extendsText = squash(match[3] ?? '').replace(/^extends\s*/, '');
    const braceIndex = match.index + match[0].length - 1;
    const endIndex = matchBracket(structural, braceIndex);
    if (endIndex < 0) continue;
    const declIndent = lineIndent(structural, match.index + match[0].indexOf('interface'));
    const fieldIndent = ' '.repeat(declIndent + 4);
    const bodyStructural = structural.slice(braceIndex + 1, endIndex);
    const bodyRaw = commentFree.slice(braceIndex + 1, endIndex);
    const lines = bodyStructural.split('\n');
    const offsets = lineOffsets(bodyStructural);
    const fields = [];
    const seen = new Set();
    for (let i = 0; i < lines.length; i += 1) {
      if (!lines[i].startsWith(fieldIndent)) continue;
      const rest = lines[i].slice(fieldIndent.length);
      if (rest.startsWith(' ') || rest.startsWith('\t')) continue;
      const m = /^(?:readonly\s+)?(?:([A-Za-z_$][\w$]*)|'([^']+)'|"([^"]+)")\s*(\??)\s*:/.exec(rest);
      if (!m) continue;
      const fieldName = m[1] ?? m[2] ?? m[3];
      if (!fieldName || fieldName.startsWith('[') || seen.has(fieldName)) continue;
      seen.add(fieldName);
      // 类型文本：从 ':' 到同层的 ';'（支持 `logit_bias: { [k: string]: number }` 这类内联对象）
      const fieldStart = offsets[i] + fieldIndent.length;
      const colonAbs = bodyStructural.indexOf(':', fieldStart);
      let valueEnd = -1;
      if (colonAbs >= 0) {
        let depth = 0;
        for (let j = colonAbs + 1; j < bodyStructural.length; j += 1) {
          const c = bodyStructural[j];
          if (c === '(' || c === '{' || c === '[' || c === '<') depth += 1;
          else if (c === ')' || c === '}' || c === ']' || c === '>') {
            if (depth === 0) break;
            depth -= 1;
          } else if (c === ';' && depth === 0) {
            valueEnd = j;
            break;
          }
        }
      }
      const typeText = valueEnd > colonAbs
        ? squash(bodyRaw.slice(colonAbs + 1, valueEnd))
        : squash(((bodyRaw.split('\n')[i] ?? '').split(':').slice(1).join(':') || '')).replace(/;$/, '');
      fields.push({ name: fieldName, optional: m[4] === '?', type: typeText });
    }
    found.push({ name, extendsText, fields });
  }
  return found;
}

// ---------------------------------------------------------------- 方法声明

// `export type XParams = A | B;` 这类请求体别名（无 interface 体，字段由成员接口承载）。
function extractTypeAliases(raw) {
  const commentFree = scanBlank(raw, { blankStrings: false });
  const re = /(?:^|\n)export\s+type\s+([A-Za-z_$][\w$]*)\s*=/g;
  const names = [];
  let match;
  while ((match = re.exec(commentFree)) !== null) names.push(match[1]);
  return names;
}

// 在 class body 内枚举 4 空格缩进的方法声明；index 偏移与原始文件对齐。
// bodyHints: method -> 该方法的请求体实参名（由 .js 的 `body:` / 简写 `body` 推断）。
function extractMethodsInClass(fileRaw, classBodyStart, classBodyEnd, bodyHints = new Map()) {
  const raw = fileRaw;
  const structural = scanBlank(raw);
  const bodyStart = classBodyStart + 1;
  const bodyStructural = structural.slice(bodyStart, classBodyEnd);
  const offsets = lineOffsets(bodyStructural);
  const lines = bodyStructural.split('\n');
  const methods = [];
  const seen = new Set();
  for (let i = 0; i < lines.length; i += 1) {
    if (!/^ {4}[A-Za-z_$][\w$]*[\s<(]/.test(lines[i])) continue;
    const start = offsets[i] + 4;
    if (seen.has(start)) continue;
    const head = /^([A-Za-z_$][\w$]*)/.exec(lines[i].slice(4));
    if (!head) continue;
    const methodName = head[1];
    if (['constructor', 'if', 'for', 'while', 'switch', 'return', 'function', 'new'].includes(methodName)) continue;

    // 跳过泛型（可嵌套：`parse<Params, ParsedT = F<Params>>(`），定位参数左括号。
    // 遇到 `:`/`=`/`;` 说明这是属性声明而不是方法，直接跳过。
    let cursor = 4 + head[0].length;
    let angle = 0;
    let parenInLine = -1;
    while (cursor < lines[i].length) {
      const ch = lines[i][cursor];
      if (ch === '<') angle += 1;
      else if (ch === '>' && lines[i][cursor - 1] !== '=') angle -= 1;
      else if (angle <= 0 && (ch === ':' || ch === '=' || ch === ';' || ch === ',')) break;
      else if (ch === '(' && angle <= 0) {
        parenInLine = cursor;
        break;
      }
      cursor += 1;
    }
    if (parenInLine < 0) continue;

    // 签名结束的 ';'（跳过跨行的括号/尖括号内部；`=>` 的 > 不计入）
    let depth = 0;
    let end = -1;
    for (let j = start; j < bodyStructural.length; j += 1) {
      const c = bodyStructural[j];
      if (c === '(' || c === '{' || c === '[' || c === '<') depth += 1;
      else if (c === '>' && bodyStructural[j - 1] === '=') {
        // arrow `=>`，不算括号
      } else if (c === ')' || c === '}' || c === ']' || c === '>') depth -= 1;
      else if (c === ';' && depth === 0) {
        end = j;
        break;
      }
    }
    if (end < 0) continue;
    seen.add(start);

    const parenOpen = offsets[i] + parenInLine;
    const parenClose = matchBracket(bodyStructural, parenOpen);
    if (parenClose < 0) continue;
    const paramsStructural = bodyStructural.slice(parenOpen + 1, parenClose);
    const paramsRaw = raw.slice(bodyStart + parenOpen + 1, bodyStart + parenClose);
    const after = bodyStructural.slice(parenClose + 1);
    const colonMatch = /^\s*:\s*/.exec(after);
    const returns = colonMatch
      ? squash(raw.slice(bodyStart + parenClose + 1 + colonMatch[0].length, bodyStart + end))
      : null;

    // 参数拆解：请求体 = 名为 `body` 的参数；或 .js 里显式 `body: <arg>` 指向的参数。
    // 本 SDK 里 `params` 既可能是 body（如 beta.threads.runs.create）也可能只是
    // query/path 参数（如 roles.delete），因此必须用 .js 的实际用法判定。
    const hint = bodyHints.get(methodName) ?? 'body';
    const genericText = lines[i]
      .slice(4 + head[0].length, parenInLine)
      .replace(/^\s*</, '')
      .replace(/>\s*$/, '');
    const genericConstraints = new Map();
    for (const part of splitTopLevel(genericText, ',')) {
      const m = /^\s*([A-Za-z_$][\w$]*)\s+extends\s+(.+)$/s.exec(part);
      if (m) genericConstraints.set(m[1], squash(m[2]));
    }
    const partsStructural = splitTopLevel(paramsStructural, ',');
    const partsRaw = splitTopLevel(paramsRaw, ',');
    let body = null;
    let positional = 0;
    for (let k = 0; k < partsStructural.length; k += 1) {
      const m = /^\s*([A-Za-z_$][\w$]*)\s*(\??)\s*:/.exec(partsStructural[k]);
      if (!m) continue;
      const argName = m[1];
      if (argName === hint) {
        const partRaw = (partsRaw[k] ?? '').trim();
        let typeRaw = squash(partRaw.slice(partRaw.indexOf(':') + 1));
        let name = leadingTypeName(typeRaw);
        // `body: Params` 且 Params 是泛型参数时，用其 extends 约束（如 ChatCompletionParseParams）
        if (genericConstraints.has(name)) {
          typeRaw = `${genericConstraints.get(name)} (generic ${name})`;
          name = leadingTypeName(genericConstraints.get(name));
        }
        body = { type: typeRaw, name, optional: m[2] === '?' };
        continue;
      }
      if (argName !== 'options' && argName !== 'query' && argName !== 'params' && !argName.endsWith('Options')) {
        positional += 1;
      }
    }
    methods.push({
      method: methodName,
      bodyType: body ? body.type : null,
      params: body ? body.name : null,
      paramsOptional: body ? body.optional : false,
      returns,
      positional,
    });
    i = bodyStructural.slice(0, end).split('\n').length - 1;
  }
  return methods;
}

// 找 class 声明并返回 body 的 [start, end]；找不到返回 null。
function findClassBody(structural, className) {
  const re = new RegExp(`export\\s+declare\\s+class\\s+${className}\\s+extends\\s+(?:[A-Za-z_$][\\w$]*\\.)?APIResource\\b[^{]*\\{`);
  const match = re.exec(structural);
  if (!match) return null;
  const braceIndex = match.index + match[0].length - 1;
  const endIndex = matchBracket(structural, braceIndex);
  if (endIndex < 0) return null;
  return { start: braceIndex, end: endIndex };
}

// ------------------------------------------------------------------- HTTP

const VERBS = ['get', 'post', 'put', 'patch', 'delete', 'getAPIList'];

// 从同名 .js 的类里取 method -> "VERB /path"
function extractHttpMap(jsRaw, className) {
  const structural = scanBlank(jsRaw);
  const map = new Map();
  const classRe = new RegExp(`(?:^|\\n)class\\s+${className}\\s+extends\\s+(?:[A-Za-z_$][\\w$]*\\.)?APIResource\\b[^{]*\\{`);
  const cls = classRe.exec(structural);
  if (!cls) return map;
  const braceIndex = cls.index + cls[0].length - 1;
  const endIndex = matchBracket(structural, braceIndex);
  if (endIndex < 0) return map;
  const bodyStart = braceIndex + 1;
  const bodyStructural = structural.slice(bodyStart, endIndex);
  const lines = bodyStructural.split('\n');
  const offsets = lineOffsets(bodyStructural);
  for (let i = 0; i < lines.length; i += 1) {
    const head = /^ {4}(?:async\s+)?([A-Za-z_$][\w$]*)\s*\(/.exec(lines[i]);
    if (!head) continue;
    const name = head[1];
    if (name === 'constructor') continue;
    const start = offsets[i];
    const parenOpen = bodyStructural.indexOf('(', start);
    const parenClose = matchBracket(bodyStructural, parenOpen);
    if (parenClose < 0) continue;
    const braceOpen = bodyStructural.indexOf('{', parenClose);
    if (braceOpen < 0) continue;
    const braceClose = matchBracket(bodyStructural, braceOpen);
    if (braceClose < 0) continue;
    const methodRaw = jsRaw.slice(bodyStart + braceOpen, bodyStart + braceClose);

    // SDK 里少数方法用 getAPIList + `method: 'post'` 覆盖真实动词（例如批量 activate），
    // 因此显式 option 优先于 helper 名。
    const override = /method:\s*'(get|post|put|patch|delete)'/.exec(methodRaw);
    let found = null;
    let bodyArg = null;
    for (const verb of VERBS) {
      const callRe = new RegExp(`this\\._client\\s*\\.\\s*${verb}\\s*\\(`);
      const call = callRe.exec(methodRaw);
      if (!call) continue;
      const argOpen = call.index + call[0].length - 1;
      const argClose = matchBracket(methodRaw, argOpen);
      if (argClose < 0) continue;
      const argsRaw = methodRaw.slice(argOpen + 1, argClose);
      const path = firstPathLiteral(argsRaw);
      if (!path) continue;
      const httpVerb = override ? override[1].toUpperCase() : verb === 'getAPIList' ? 'GET' : verb.toUpperCase();
      found = `${httpVerb} ${path}`;
      // 请求体判定：options 对象里出现 `body` 键。
      //   `body: params`  -> 实参名 params
      //   简写 `body`     -> 通常是同名形参；若方法内先 `const { ...body... } = params`
      //                      解构再简写（beta.threads.runs.create 即如此），则归到 params。
      const bodyRef = /(?:^|[,{(\s])body\s*(?=,|:|\)|\}|$)/.exec(argsRaw);
      if (bodyRef) {
        const afterBody = /^\s*:\s*([A-Za-z_$][\w$]*)/.exec(argsRaw.slice(bodyRef.index + bodyRef[0].length - 1));
        if (afterBody) bodyArg = afterBody[1];
        else {
          const destructured = /const\s*\{[^}]*\bbody\b[^}]*\}\s*=\s*([A-Za-z_$][\w$]*)/.exec(methodRaw);
          bodyArg = destructured ? destructured[1] : 'body';
        }
      }
      break;
    }
    if (!map.has(name)) map.set(name, found === null ? null : { http: found, bodyArg });
  }
  return map;
}

// 取参数表达式里第一个字符串/模板字面量；模板插值 ${x} -> {x}
function firstPathLiteral(text) {
  let i = 0;
  while (i < text.length) {
    const c = text[i];
    if (c === "'" || c === '"') {
      let j = i + 1;
      let value = '';
      while (j < text.length && text[j] !== c) {
        if (text[j] === '\\') {
          value += text[j + 1] ?? '';
          j += 2;
          continue;
        }
        value += text[j];
        j += 1;
      }
      return value;
    }
    if (c === '`') {
      let j = i + 1;
      let value = '';
      while (j < text.length && text[j] !== '`') {
        if (text[j] === '\\') {
          value += text[j + 1] ?? '';
          j += 2;
          continue;
        }
        if (text[j] === '$' && text[j + 1] === '{') {
          const close = matchBracket(text, j + 1);
          if (close < 0) return null;
          value += `{${squash(text.slice(j + 2, close))}}`;
          j = close + 1;
          continue;
        }
        value += text[j];
        j += 1;
      }
      return value;
    }
    i += 1;
  }
  return null;
}

// ------------------------------------------------------------------- 主流程

function main() {
  const args = parseArgs(process.argv);
  const sdkDir = args.sdk;
  const outDir = args.out;
  mkdirSync(outDir, { recursive: true });

  const pkg = JSON.parse(readFileSync(join(sdkDir, 'package.json'), 'utf8'));
  const version = pkg.version ?? 'unknown';

  const declFiles = walk(sdkDir, (p) => p.endsWith('.d.ts')).map((p) => toPosix(relative(sdkDir, p)));
  const jsFiles = new Set(walk(sdkDir, (p) => p.endsWith('.js')).map((p) => toPosix(relative(sdkDir, p))));

  // 1) 收集所有 interface。同名接口可能声明在多个 resource 模块里（例如
  //    containers.files 与 vector_stores.files 各有自己的 FileCreateParams），
  //    因此以 name + 声明文件为键，逐声明保留，绝不合并丢失字段。
  const allInterfaces = new Map(); // "name::file" -> {name, file, fields, extendsText}
  const allAliases = new Map(); // "name::file" -> {name, file}
  for (const relPath of declFiles) {
    const raw = readFileSync(join(sdkDir, relPath), 'utf8');
    for (const iface of extractInterfaces(raw)) {
      const key = `${iface.name}::${relPath}`;
      const existing = allInterfaces.get(key);
      if (existing) {
        // 同一文件内重复声明：取字段并集，保持顺序
        const names = new Set(existing.fields.map((f) => f.name));
        for (const field of iface.fields) if (!names.has(field.name)) existing.fields.push(field);
        continue;
      }
      allInterfaces.set(key, { ...iface, file: relPath });
    }
    for (const name of extractTypeAliases(raw)) {
      allAliases.set(`${name}::${relPath}`, { name, file: relPath });
    }
  }
  const interfacesByName = new Map();
  for (const iface of allInterfaces.values()) {
    if (!interfacesByName.has(iface.name)) interfacesByName.set(iface.name, []);
    interfacesByName.get(iface.name).push(iface);
  }

  // 2) 解析 resource 类与带请求体的方法
  const methods = [];
  const methodsWithoutBody = [];
  const classFiles = [];
  for (const relPath of declFiles.filter((p) => p.startsWith('resources/'))) {
    const raw = readFileSync(join(sdkDir, relPath), 'utf8');
    if (!/export\s+declare\s+class\s+[A-Za-z_$][\w$]*\s+extends\s+(?:[A-Za-z_$][\w$]*\.)?APIResource\b/.test(raw)) continue;
    const className = /export\s+declare\s+class\s+([A-Za-z_$][\w$]*)\s+extends/.exec(raw)[1];
    const structural = scanBlank(raw);
    const body = findClassBody(structural, className);
    if (!body) {
      process.stderr.write(`warn: class body not found in ${relPath}\n`);
      continue;
    }
    const resource = resourceNameFromPath(relPath);
    const jsRel = relPath.replace(/\.d\.ts$/, '.js');
    const jsInfo = jsFiles.has(jsRel) ? extractHttpMap(readFileSync(join(sdkDir, jsRel), 'utf8'), className) : new Map();
    const bodyHints = new Map();
    for (const [methodName, info] of jsInfo) {
      if (info && info.bodyArg) bodyHints.set(methodName, info.bodyArg);
    }
    const classMethods = extractMethodsInClass(raw, body.start, body.end, bodyHints);
    classFiles.push({ resource, className, file: relPath, methodCount: classMethods.length });

    for (const m of classMethods) {
      if (!m.params) {
        methodsWithoutBody.push({ resource, method: m.method, returns: m.returns ?? '', file: relPath });
        continue;
      }
      methods.push({
        resource,
        method: m.method,
        params: m.params,
        params_raw: m.bodyType,
        params_optional: m.paramsOptional,
        returns: m.returns ?? 'unknown',
        http: jsInfo.get(m.method)?.http ?? null,
        positional_params: m.positional,
        file: relPath,
        className,
      });
    }
  }

  // 3) 同名方法的重载合并（create 常有 NonStreaming/Streaming/Base 三个重载）
  const merged = new Map();
  for (const m of methods) {
    const key = `${m.resource}#${m.method}`;
    const target = merged.get(key);
    if (!target) {
      merged.set(key, { ...m, overloads: [m.params_raw], returnsList: [m.returns] });
      continue;
    }
    if (!target.overloads.includes(m.params_raw)) target.overloads.push(m.params_raw);
    if (!target.returnsList.includes(m.returns)) target.returnsList.push(m.returns);
    if (/ParamsBase$/.test(m.params) && !/ParamsBase$/.test(target.params)) {
      target.params = m.params;
      target.params_raw = m.params_raw;
    }
  }
  const methodList = [...merged.values()]
    .map((m) => ({
      resource: m.resource,
      method: m.method,
      params: m.params,
      params_raw: m.params_raw,
      params_optional: m.params_optional,
      returns: m.returnsList.join(' | '),
      http: m.http,
      positional_params: m.positional_params,
      overloads: m.overloads,
      file: m.file,
    }))
    .sort((a, b) => (a.resource === b.resource ? a.method.localeCompare(b.method) : a.resource.localeCompare(b.resource)));

  // 4) 类型清单：名字含 Params 的接口 + 任何被当作请求体的具名类型。
  //    逐声明（name + file）输出：同名接口在不同 resource 模块里字段可能不同。
  //    `export type XParams = A | B` 这类别名无 interface 体，保留名字并标 unresolved，
  //    其成员接口（A/B）本身若以 Params 结尾会作为独立条目出现。
  const bodyTypeNames = new Set(methodList.map((m) => m.params).filter((n) => interfacesByName.has(n)));
  const interfaceEntries = [...allInterfaces.values()]
    .filter((t) => /Params/.test(t.name) || bodyTypeNames.has(t.name))
    .map((t) => ({
      name: t.name,
      kind: 'interface',
      file: t.file,
      extends: t.extendsText || null,
      fields: t.fields.map((f) => f.name),
      field_details: t.fields,
    }));
  const interfaceKeys = new Set(interfaceEntries.map((t) => `${t.name}::${t.file}`));
  const aliasEntries = [...allAliases.values()]
    .filter((t) => /Params/.test(t.name) && !interfaceKeys.has(`${t.name}::${t.file}`))
    .map((t) => ({
      name: t.name,
      kind: 'type_alias',
      file: t.file,
      extends: null,
      unresolved: true,
      fields: [],
      field_details: [],
    }));
  const typeList = [...interfaceEntries, ...aliasEntries].sort((a, b) =>
    a.name === b.name ? a.file.localeCompare(b.file) : a.name.localeCompare(b.name),
  );
  const duplicateTypeNames = [...new Set(typeList.map((t) => t.name))].filter(
    (name) => typeList.filter((t) => t.name === name).length > 1,
  );
  const interfaceNames = new Set(interfaceEntries.map((t) => t.name));
  const unresolvedParamNames = [...new Set(methodList.map((m) => m.params))].filter((n) => !interfaceNames.has(n));

  // 5) 时间戳：显式参数 > SOURCE_DATE_EPOCH > 当前时间
  let generatedAt;
  if (args.generatedAt) generatedAt = args.generatedAt;
  else if (process.env.SOURCE_DATE_EPOCH) {
    generatedAt = new Date(Number(process.env.SOURCE_DATE_EPOCH) * 1000).toISOString().replace(/\.\d{3}Z$/, 'Z');
  } else generatedAt = new Date().toISOString().replace(/\.\d{3}Z$/, 'Z');

  const resourceCount = new Set(methodList.map((m) => m.resource)).size;
  const httpResolved = methodList.filter((m) => m.http).length;
  for (const m of methodList) m.params_resolved = interfaceNames.has(m.params);
  const manifest = {
    provider: 'openai',
    package: pkg.name,
    version,
    generated_at: generatedAt,
    source: `https://registry.npmjs.org/${pkg.name}`,
    coverage: {
      resources: resourceCount,
      methods: methodList.length,
      params_types: typeList.length,
    },
    sdk_class_files: classFiles.length,
    methods_without_body: methodsWithoutBody.length,
    http_resolved: httpResolved,
    http_unresolved: methodList.length - httpResolved,
    params_interfaces: interfaceEntries.length,
    params_type_aliases: aliasEntries.length,
    params_types_with_duplicate_name: duplicateTypeNames.length,
    unresolved_params_types: unresolvedParamNames,
    method_listing_rule: '只收录带 `body` 参数的方法；无请求体（仅 path/query）的方法计入 methods_without_body',
    types_note: '同名 Params 接口在不同 resource 模块里可能分别声明，按 name+file 逐条列出；type alias 无字段，标注 unresolved',
    methods: methodList.map(({ file, ...rest }) => ({ ...rest, file })),
    types: typeList,
  };
  writeFileSync(join(outDir, 'manifest.json'), `${JSON.stringify(manifest, null, 2)}\n`);

  // 6) SUMMARY.md
  const byResource = new Map();
  for (const m of methodList) {
    if (!byResource.has(m.resource)) byResource.set(m.resource, []);
    byResource.get(m.resource).push(m);
  }
  const typeByName = new Map();
  for (const t of typeList) {
    if (!typeByName.has(t.name)) typeByName.set(t.name, []);
    typeByName.get(t.name).push(t);
  }
  // 同一资源模块内优先取本文件的同名声明，否则取字段最多的那条
  const lookupType = (name, file) => {
    const candidates = typeByName.get(name);
    if (!candidates) return null;
    const sameFile = candidates.find((t) => t.file === file);
    if (sameFile) return sameFile;
    return [...candidates].sort((a, b) => b.fields.length - a.fields.length)[0];
  };
  const lines = [];
  lines.push(`# OpenAI SDK API 清单（${pkg.name}@${version}）`);
  lines.push('');
  lines.push(`- 生成时间（UTC）：\`${generatedAt}\``);
  lines.push(`- 来源：npm \`${pkg.name}@${version}\` tarball，静态解析 \`.d.ts\` / \`.js\`，未执行上游代码`);
  lines.push(
    `- 覆盖：**${resourceCount}** 个 resource、**${methodList.length}** 个带请求体的方法、**${typeList.length}** 个请求体类型（interface ${interfaceEntries.length} + 类型别名 ${aliasEntries.length}）`,
  );
  lines.push(
    `- 另有 **${methodsWithoutBody.length}** 个无请求体的方法（list / retrieve / delete / cancel 等，只有 path 或 query 参数）未列入下表`,
  );
  lines.push('- `HTTP` 列由 `resources/**/*.js` 中 `this._client.<verb>(...)` 的字面量静态推导，推导不出时为 `-`');
  lines.push(
    `- HTTP 可推导 ${httpResolved}/${methodList.length}；其余方法委托给 ` +
      '`lib/*.js` 辅助函数或其它 resource（如 `embeddings.create`），静态不可见，保留 `-`',
  );
  lines.push('- 同一方法的重载（如 `create` 的 NonStreaming / Streaming / Base）合并为一行，`Params` 优先取 `*ParamsBase`');
  if (unresolvedParamNames.length > 0) {
    lines.push(
      `- \`unresolved\` 的 Params 是 \`export type\` 别名或内联对象，无 interface 字段表：` +
        unresolvedParamNames.map((n) => `\`${n}\``).join('、'),
    );
  }
  lines.push('');
  lines.push('## 资源总览');
  lines.push('');
  lines.push('| Resource | 带请求体方法数 | 可静态推导 HTTP |');
  lines.push('| --- | ---: | ---: |');
  for (const [resource, items] of [...byResource.entries()].sort((a, b) => a[0].localeCompare(b[0]))) {
    lines.push(`| \`${resource}\` | ${items.length} | ${items.filter((i) => i.http).length} |`);
  }
  lines.push('');
  lines.push('## 方法明细');
  for (const [resource, items] of [...byResource.entries()].sort((a, b) => a[0].localeCompare(b[0]))) {
    lines.push('');
    lines.push(`### \`${resource}\``);
    lines.push('');
    lines.push('| 方法 | Params | HTTP | 顶层参数数 | 返回类型 |');
    lines.push('| --- | --- | --- | ---: | --- |');
    for (const m of items) {
      const t = lookupType(m.params, m.file);
      const count = m.params_resolved && t ? t.fields.length : '—';
      const returns = m.returns.replace(/\|/g, '\\|').replace(/`/g, "'");
      const paramsCell = m.params_resolved ? `\`${m.params}\`` : `\`${m.params}\` (unresolved)`;
      lines.push(
        `| \`${m.method}\` | ${paramsCell} | ${m.http ? `\`${m.http}\`` : '-'} | ${count} | \`${returns}\` |`,
      );
    }
  }
  lines.push('');
  lines.push('## 请求体类型（*Params）');
  lines.push('');
  lines.push('| 类型 | 声明文件 | 顶层字段数 | 顶层字段 |');
  lines.push('| --- | --- | ---: | --- |');
  for (const t of typeList) {
    const fieldText = t.fields.length ? t.fields.map((f) => `\`${f}\``).join(', ') : '—';
    lines.push(`| \`${t.name}\` | \`${t.file}\` | ${t.fields.length} | ${fieldText} |`);
  }
  lines.push('');
  writeFileSync(join(outDir, 'SUMMARY.md'), lines.join('\n'));

  // 7) 上游自带的 api.md：顶层没有时，把 src/resources/<name>/api.md 按 resource 拼接
  const upstreamMd = join(outDir, 'upstream-api.md');
  const topLevelApiMd = join(sdkDir, 'api.md');
  const resourceApiMds = walk(sdkDir, (p) => /(^|\/)src\/resources\/[^/]+\/api\.md$/.test(toPosix(p)));
  if (existsSync(topLevelApiMd)) {
    writeFileSync(upstreamMd, readFileSync(topLevelApiMd, 'utf8'));
    process.stdout.write('extract-api: upstream-api.md <- api.md\n');
  } else if (resourceApiMds.length > 0) {
    const chunks = [
      `<!-- 由 scripts/extract-api.mjs 从 ${pkg.name}@${version} 的 src/resources/*/api.md 拼接，仅作参照 -->`,
      '',
    ];
    for (const file of resourceApiMds) {
      const rel = toPosix(relative(sdkDir, file));
      const resource = rel.split('/')[2] ?? rel;
      chunks.push(`<!-- source: ${rel} -->`, '', `## ${resource}`, '', readFileSync(file, 'utf8').trimEnd(), '');
    }
    writeFileSync(upstreamMd, `${chunks.join('\n')}\n`);
    process.stdout.write(`extract-api: upstream-api.md <- ${resourceApiMds.length} 个 resource api.md\n`);
  } else {
    process.stdout.write('extract-api: 上游未提供 api.md，跳过 upstream-api.md\n');
  }

  process.stdout.write(
    `extract-api: ${pkg.name}@${version} resources=${resourceCount} methods=${methodList.length} ` +
      `params_types=${typeList.length} no_body_methods=${methodsWithoutBody.length} ` +
      `dup_type_names=${duplicateTypeNames.length}\n`,
  );
}

main();
