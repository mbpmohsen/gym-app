// Theme = mode (light / dark / system) × accent color. A per-computer UI
// preference, so it lives in localStorage (index.html applies it before first
// paint to avoid a flash). If storage is unavailable the defaults are used.

import { useSyncExternalStore } from 'react'

export type Mode = 'light' | 'dark' | 'system'
export type Accent = 'blue' | 'emerald' | 'violet' | 'orange' | 'rose' | 'neutral'

export const ACCENTS: { id: Accent; label: string; swatch: string }[] = [
  { id: 'blue', label: 'آبی', swatch: 'oklch(0.546 0.245 262.881)' },
  { id: 'emerald', label: 'سبز', swatch: 'oklch(0.596 0.145 163.225)' },
  { id: 'violet', label: 'بنفش', swatch: 'oklch(0.541 0.281 293.009)' },
  { id: 'orange', label: 'نارنجی', swatch: 'oklch(0.646 0.222 41.116)' },
  { id: 'rose', label: 'سرخابی', swatch: 'oklch(0.586 0.253 17.585)' },
  { id: 'neutral', label: 'خنثی', swatch: 'oklch(0.3 0 0)' },
]

export type Theme = { mode: Mode; accent: Accent }
const KEY = 'gym-theme'
const DEFAULT: Theme = { mode: 'light', accent: 'blue' }

function read(): Theme {
  try {
    const t = JSON.parse(localStorage.getItem(KEY) ?? 'null')
    if (t && ['light', 'dark', 'system'].includes(t.mode) && ACCENTS.some((a) => a.id === t.accent)) return t
  } catch {
    /* storage blocked or corrupt: defaults */
  }
  return DEFAULT
}

let current = read()
const listeners = new Set<() => void>()
const media = window.matchMedia('(prefers-color-scheme: dark)')

function apply(t: Theme) {
  const dark = t.mode === 'dark' || (t.mode === 'system' && media.matches)
  const el = document.documentElement
  el.classList.toggle('dark', dark)
  el.dataset.accent = t.accent
  el.style.colorScheme = dark ? 'dark' : 'light'
}

media.addEventListener('change', () => apply(current))
apply(current)

export function setTheme(patch: Partial<Theme>) {
  current = { ...current, ...patch }
  try {
    localStorage.setItem(KEY, JSON.stringify(current))
  } catch {
    /* not persisted, still applied */
  }
  apply(current)
  listeners.forEach((l) => l())
}

export function useTheme(): Theme {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l)
      return () => listeners.delete(l)
    },
    () => current,
  )
}
