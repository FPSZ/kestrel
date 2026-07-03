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

// --- Locale-aware formatting (foundations #8) --------------------------------
// Timestamps are stored/transported as epoch millis (UTC, timezone-agnostic);
// formatting to the viewer's locale + timezone happens here at the edge via Intl.
// Keyed off the app locale (getLocale()), not the browser default, so a /lang
// override formats consistently with the rest of the UI.

/** Locale-aware wall-clock time (24h), e.g. "14:07:32" / "14:07:32". */
export function formatTime(ts: number): string {
  return new Intl.DateTimeFormat(locale, {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hour12: false,
  }).format(new Date(ts))
}

/** Locale-aware date + time, for replay headers and the like. */
export function formatDateTime(ts: number): string {
  return new Intl.DateTimeFormat(locale, {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
    hour12: false,
  }).format(new Date(ts))
}

/** Locale-aware number with grouping, e.g. 3200 -> "3,200" / "3,200". */
export function formatNumber(n: number): string {
  return new Intl.NumberFormat(locale).format(n)
}
