// Persian (Jalali) dates and Persian digits via the built-in Intl API: no extra library.
// The server stores ISO dates ("2026-10-09", "2026-10-09 18:30:00", local time).

const dateFmt = new Intl.DateTimeFormat('fa-IR-u-ca-persian', { year: 'numeric', month: 'long', day: 'numeric' })
const shortDateFmt = new Intl.DateTimeFormat('fa-IR-u-ca-persian', { year: 'numeric', month: '2-digit', day: '2-digit' })
const timeFmt = new Intl.DateTimeFormat('fa-IR', { hour: '2-digit', minute: '2-digit', hour12: false })
const weekdayFmt = new Intl.DateTimeFormat('fa-IR-u-ca-persian', { weekday: 'long' })
const numFmt = new Intl.NumberFormat('fa-IR')

/** "2026-10-09" or "2026-10-09 18:30:00" -> local Date */
export function parseIso(s: string): Date {
  const [d, t = '00:00:00'] = s.split(/[ T]/)
  const [y, m, day] = d.split('-').map(Number)
  const [hh, mm, ss] = t.split(':').map(Number)
  return new Date(y, m - 1, day, hh, mm, ss || 0)
}

export const jalaliDate = (d: Date | string) => dateFmt.format(typeof d === 'string' ? parseIso(d) : d)
export const jalaliShort = (d: Date | string) => shortDateFmt.format(typeof d === 'string' ? parseIso(d) : d)
export const time = (d: Date | string) => timeFmt.format(typeof d === 'string' ? parseIso(d) : d)
export const weekday = (d: Date | string) => weekdayFmt.format(typeof d === 'string' ? parseIso(d) : d)
export const num = (n: number) => numFmt.format(n)
export const toman = (n: number) => `${numFmt.format(n)} تومان`

/** ASCII digits -> Persian digits, keeping everything else (phones, codes). */
export const faDigits = (s: string) => s.replace(/\d/g, (d) => '۰۱۲۳۴۵۶۷۸۹'[Number(d)])
