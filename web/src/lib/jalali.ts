// Jalali <-> ISO (Gregorian "YYYY-MM-DD"), for date inputs. Display formatting lives in format.ts.

import { isValidJalaaliDate, toGregorian, toJalaali } from 'jalaali-js'

const pad = (n: number) => String(n).padStart(2, '0')

/** Persian / Arabic digits -> ASCII */
export function asciiDigits(s: string): string {
  return s.replace(/[۰-۹]/g, (d) => String(d.charCodeAt(0) - 0x06f0)).replace(/[٠-٩]/g, (d) => String(d.charCodeAt(0) - 0x0660))
}

/** "1405/07/17" (any digits, / - . or space) -> "2026-10-09"; null if invalid. */
export function jalaliToIso(input: string): string | null {
  const parts = asciiDigits(input).trim().split(/[/\-.\s]+/).map(Number)
  if (parts.length !== 3 || parts.some((p) => !Number.isInteger(p))) return null
  let [jy] = parts
  const [, jm, jd] = parts
  if (jy < 100) jy += 1300 // "05/07/17" -> 1405
  if (!isValidJalaaliDate(jy, jm, jd)) return null
  const g = toGregorian(jy, jm, jd)
  return `${g.gy}-${pad(g.gm)}-${pad(g.gd)}`
}

/** "2026-10-09" -> "1405/07/17" (ASCII digits, for editing) */
export function isoToJalali(iso: string): string {
  const [y, m, d] = iso.slice(0, 10).split('-').map(Number)
  const j = toJalaali(y, m, d)
  return `${j.jy}/${pad(j.jm)}/${pad(j.jd)}`
}

export function todayIso(): string {
  const n = new Date()
  return `${n.getFullYear()}-${pad(n.getMonth() + 1)}-${pad(n.getDate())}`
}
