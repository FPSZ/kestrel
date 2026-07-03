// i18n 硬编码门禁（地基 #1 / ADR-0008，看板 G8）。
//
// 扫描 console 源码里**硬编码的 CJK 字符**——最典型、最高频的漏翻信号：用户可见文本
// 必须走共享 catalog（src/i18n/*.json）或页面级 catalog（features/launcher/strings.ts），
// 不得内联进组件。注释里允许中文（开发注释不面向用户），故先按行剥掉注释再判。
//
// 取舍（诚实标注）：零依赖、确定性、只堵"硬编码 CJK"这一主回归。更强的
// no-literal-string（连英文字面量也强制走 t()）噪声大、需大量 allowlist，留待引入
// ESLint 后再加。本门禁不 string-aware：串里出现 `//` 会把其后内容当注释丢弃，可能
// 漏判极少数边角，但绝不误报——宁可漏一个也不卡正常提交。

import { readdirSync, readFileSync, statSync } from 'node:fs'
import { join, relative } from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = fileURLToPath(new URL('..', import.meta.url)) // console/
const SRC = join(ROOT, 'src')

// 允许含 CJK 的文件：翻译 catalog 本身、页面级 catalog。
const ALLOW = [/[\\/]i18n[\\/]/, /[\\/]launcher[\\/]strings\.ts$/]
// CJK 统一表意 + 扩展A + 兼容表意 + 日文假名（够覆盖中日文 UI 文本）。
const CJK = /[぀-ヿ㐀-䶿一-鿿豈-﫿]/

function walk(dir) {
  const out = []
  for (const name of readdirSync(dir)) {
    const p = join(dir, name)
    if (statSync(p).isDirectory()) out.push(...walk(p))
    else if (/\.(ts|tsx)$/.test(p)) out.push(p)
  }
  return out
}

// 剥注释但保留行数（块注释可跨行）：行注释丢弃 `//` 之后；块注释 `/* */` 内容清空。
function stripComments(code) {
  let inBlock = false
  return code
    .split('\n')
    .map((line) => {
      let out = ''
      let i = 0
      while (i < line.length) {
        if (inBlock) {
          const end = line.indexOf('*/', i)
          if (end === -1) {
            i = line.length
          } else {
            inBlock = false
            i = end + 2
          }
        } else if (line.startsWith('/*', i)) {
          inBlock = true
          i += 2
        } else if (line.startsWith('//', i)) {
          break // 本行余下是注释
        } else {
          out += line[i]
          i += 1
        }
      }
      return out
    })
    .join('\n')
}

const violations = []
for (const file of walk(SRC)) {
  if (ALLOW.some((re) => re.test(file))) continue
  stripComments(readFileSync(file, 'utf8'))
    .split('\n')
    .forEach((line, i) => {
      if (CJK.test(line)) {
        violations.push(`${relative(ROOT, file).replace(/\\/g, '/')}:${i + 1}  ${line.trim().slice(0, 80)}`)
      }
    })
}

if (violations.length) {
  console.error('i18n gate FAILED: hardcoded CJK in components (move UI text into a catalog):\n')
  for (const v of violations) console.error('  ' + v)
  console.error(`\n${violations.length} violation(s). See docs/adr/0008-i18n-localization.md.`)
  process.exit(1)
}
console.log('i18n gate OK: no hardcoded CJK in components.')
