// Presentation-edge i18n (ADR-0008). The event log + model-facing wire stay
// canonical English; ONLY user-facing UI strings are localized here.
//
// - English keys, en-US is the fallback catalog (missing zh-CN key -> en-US -> key).
// - `t(key, params)` interpolates {name} placeholders.
// - Locale is picked once from the browser; a future /lang command or settings
//   toggle can override (persist to localStorage) without touching call sites.

import enUS from './en-US.json'
import zhCN from './zh-CN.json'

export type Locale = 'en-US' | 'zh-CN'

type Catalog = Record<string, string>
const CATALOGS: Record<Locale, Catalog> = { 'en-US': enUS, 'zh-CN': zhCN }
const FALLBACK: Locale = 'en-US'

function detectLocale(): Locale {
  try {
    const stored = localStorage.getItem('kestrel.locale')
    if (stored === 'en-US' || stored === 'zh-CN') return stored
  } catch {
    /* storage unavailable - fall through to navigator */
  }
  const nav = typeof navigator !== 'undefined' ? navigator.language : FALLBACK
  return nav.toLowerCase().startsWith('zh') ? 'zh-CN' : FALLBACK
}

let locale: Locale = detectLocale()

/** Current UI locale. */
export function getLocale(): Locale {
  return locale
}

/** Switch locale (persisted). Callers should re-render / reload to apply everywhere. */
export function setLocale(next: Locale): void {
  locale = next
  try {
    localStorage.setItem('kestrel.locale', next)
  } catch {
    /* non-fatal */
  }
}

/**
 * Localize `key`, interpolating `{name}` placeholders from `params`.
 * Falls back en-US -> the key itself, so a missing translation is visible but
 * never blank.
 */
export function t(key: string, params?: Record<string, string | number>): string {
  let s = CATALOGS[locale][key] ?? CATALOGS[FALLBACK][key] ?? key
  if (params) {
    for (const [k, v] of Object.entries(params)) {
      s = s.replaceAll(`{${k}}`, String(v))
    }
  }
  return s
}
