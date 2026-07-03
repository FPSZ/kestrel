// Launcher page copy — centralized bilingual strings (no scattered hardcode).
//
// Kept LOCAL on purpose: the shared i18n catalogs (src/i18n/*.json) are owned/edited
// elsewhere, so this page carries its own strings under the same `launcher.*` key
// convention and reads the same `kestrel.locale` source. MIGRATION: once catalogs
// settle, move these maps into src/i18n/{en-US,zh-CN}.json and swap `tl` for `t`.

type Dict = Record<string, string>

const EN: Dict = {
  'launcher.title': 'Models',
  'launcher.subtitle':
    'Browse local GGUF models, run one as a local llama.cpp server, and see engines already running. The agent connects to whatever base_url your config points at.',
  'launcher.dir.label': 'Models folder',
  'launcher.dir.placeholder': 'path to your .gguf models',
  'launcher.rescan': 'Rescan',
  'launcher.scanning': 'Scanning…',
  'launcher.error':
    'Launcher API unavailable. Rebuild and restart kestrel-server to enable /api/launcher/*.',

  'launcher.engine.title': 'Local server',
  'launcher.engine.stopped': 'Stopped',
  'launcher.engine.loading': 'Loading',
  'launcher.engine.running': 'Running',
  'launcher.engine.failed': 'Failed',
  'launcher.engine.reachable': 'Reachable at',
  'launcher.engine.idle': 'No engine started from here yet.',
  'launcher.stop': 'Stop',

  'launcher.models.title': 'Local models',
  'launcher.models.meta': '{n} models · {size}',
  'launcher.models.empty': 'No .gguf models found in this folder.',
  'launcher.col.model': 'Model',
  'launcher.col.arch': 'Arch',
  'launcher.col.params': 'Params',
  'launcher.col.quant': 'Quant',
  'launcher.col.size': 'Size',
  'launcher.load': 'Load',
  'launcher.loadingBtn': 'Loading…',

  'launcher.bin.label': 'Engine',
  'launcher.bin.none': 'no llama-server found',
  'launcher.bin.onPath': 'on PATH',
  'launcher.needBin': 'A llama-server binary is required to run a local model.',

  'launcher.running.title': 'Running engines',
  'launcher.running.empty': 'No running engine detected on common local ports.',
  'launcher.running.use': 'Copy connect',
  'launcher.copied': 'Copied',
}

const ZH: Dict = {
  'launcher.title': '模型',
  'launcher.subtitle':
    '浏览本地 GGUF 模型、把某个跑成本地 llama.cpp 服务器、并看到已在运行的引擎。agent 连的是你配置里 base_url 指向的那个。',
  'launcher.dir.label': '模型目录',
  'launcher.dir.placeholder': '你的 gguf 模型目录',
  'launcher.rescan': '重新扫描',
  'launcher.scanning': '扫描中…',
  'launcher.error':
    '启动器接口不可用。重新构建并重启 kestrel-server 以启用 /api/launcher/*。',

  'launcher.engine.title': '本地服务器',
  'launcher.engine.stopped': '已停止',
  'launcher.engine.loading': '加载中',
  'launcher.engine.running': '运行中',
  'launcher.engine.failed': '失败',
  'launcher.engine.reachable': '可达于',
  'launcher.engine.idle': '还没有从这里启动过引擎。',
  'launcher.stop': '停止',

  'launcher.models.title': '本地模型',
  'launcher.models.meta': '{n} 个模型 · {size}',
  'launcher.models.empty': '该目录没找到 gguf 模型。',
  'launcher.col.model': '模型',
  'launcher.col.arch': '架构',
  'launcher.col.params': '参数',
  'launcher.col.quant': '量化',
  'launcher.col.size': '大小',
  'launcher.load': '加载',
  'launcher.loadingBtn': '加载中…',

  'launcher.bin.label': '引擎',
  'launcher.bin.none': '没找到 llama-server',
  'launcher.bin.onPath': '在 PATH',
  'launcher.needBin': '运行本地模型需要一个 llama-server 二进制。',

  'launcher.running.title': '运行中的引擎',
  'launcher.running.empty': '常见本地端口上没探到在跑的引擎。',
  'launcher.running.use': '复制连接',
  'launcher.copied': '已复制',
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
