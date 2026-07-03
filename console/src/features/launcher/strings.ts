// Launcher page copy — centralized bilingual strings (no scattered hardcode).
//
// Kept LOCAL on purpose: the shared i18n catalogs (src/i18n/*.json) are still
// in flight, so this page carries its own strings under the same `launcher.*`
// key convention and reads the same `kestrel.locale` source. MIGRATION: once the
// shared catalogs settle, move these maps into src/i18n/{en-US,zh-CN}.json and
// swap `tl` for the shared `t` from '@/i18n' — call sites already use keys.

type Dict = Record<string, string>

const EN: Dict = {
  'launcher.title': 'Models',
  'launcher.subtitle':
    'Engines discovered on this machine. Scanning only suggests — nothing is launched automatically; you enable a choice by saving it to a loadout.',
  'launcher.rescan': 'Rescan',
  'launcher.scanning': 'Scanning…',
  'launcher.error':
    'Scan endpoint unavailable. Rebuild and restart kestrel-server to enable /api/launcher/scan.',
  'launcher.running.title': 'Running engines',
  'launcher.running.empty': 'No running engine detected on common local ports.',
  'launcher.bin.title': 'llama-server binaries',
  'launcher.bin.empty':
    'No llama-server found on PATH or common install dirs. Set bin manually in your loadout.',
  'launcher.badge.onPath': 'on PATH',
  'launcher.use': 'Use',
  'launcher.snippet.title': 'Loadout snippet',
  'launcher.snippet.hint':
    'Paste into kestrel.loadout.toml, then set loadout = "kestrel.loadout.toml" in kestrel.toml and restart.',
  'launcher.snippet.empty': 'Pick a candidate above to generate its loadout.',
  'launcher.copy': 'Copy',
  'launcher.copied': 'Copied',
  'launcher.ctx': 'ctx {n}',
}

const ZH: Dict = {
  'launcher.title': '模型',
  'launcher.subtitle':
    '本机发现的引擎。扫描只做建议——不会自动启动任何东西；把某个选择写进 loadout 才会真正生效。',
  'launcher.rescan': '重新扫描',
  'launcher.scanning': '扫描中…',
  'launcher.error':
    '扫描接口不可用。重新构建并重启 kestrel-server 以启用 /api/launcher/scan。',
  'launcher.running.title': '运行中的引擎',
  'launcher.running.empty': '常见本地端口上没探到在跑的引擎。',
  'launcher.bin.title': 'llama-server 二进制',
  'launcher.bin.empty':
    '在 PATH 或常见目录没找到 llama-server。请在 loadout 里手填 bin 路径。',
  'launcher.badge.onPath': '在 PATH',
  'launcher.use': '使用',
  'launcher.snippet.title': 'Loadout 片段',
  'launcher.snippet.hint':
    '粘贴进 kestrel.loadout.toml，再在 kestrel.toml 里设 loadout = "kestrel.loadout.toml" 并重启。',
  'launcher.snippet.empty': '在上方选一个候选，生成它的 loadout。',
  'launcher.copy': '复制',
  'launcher.copied': '已复制',
  'launcher.ctx': 'ctx {n}',
}

function locale(): 'en-US' | 'zh-CN' {
  try {
    const s = localStorage.getItem('kestrel.locale')
    if (s === 'en-US' || s === 'zh-CN') return s
  } catch {
    /* storage unavailable */
  }
  const nav = typeof navigator !== 'undefined' ? navigator.language : 'en-US'
  return nav.toLowerCase().startsWith('zh') ? 'zh-CN' : 'en-US'
}

/** Localize a launcher key, interpolating {name} placeholders. Falls back en -> key. */
export function tl(key: string, params?: Record<string, string | number>): string {
  const cat = locale() === 'zh-CN' ? ZH : EN
  let s = cat[key] ?? EN[key] ?? key
  if (params) {
    for (const [k, v] of Object.entries(params)) {
      s = s.replaceAll(`{${k}}`, String(v))
    }
  }
  return s
}
