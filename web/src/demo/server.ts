// In-browser stand-in for gym-server, used only by the GitHub Pages demo build
// (VITE_DEMO). Mirrors the real API and its rules (SPEC §4) closely enough for
// the UI; the source of truth stays server/src/*.rs.

import { ApiError } from '@/lib/api'
import { addDays, isoDateTime, now, parseDate, seed, today, weekday, type Db, type Member, type Sub, type Visit } from './data'

const db: Db = seed()

// ---------- live events (replaces /api/live) ----------

type LiveEvent = { kind: 'entry' | 'exit' | 'face' | 'refresh'; member_id: number | null; name: string | null; status: string | null; flags: string[]; sound: string | null }
const listeners = new Set<(e: LiveEvent) => void>()
export function onLive(fn: (e: LiveEvent) => void): () => void {
  listeners.add(fn)
  fn(refresh())
  return () => listeners.delete(fn)
}
const emit = (e: LiveEvent) => listeners.forEach((l) => l(e))
const refresh = (): LiveEvent => ({ kind: 'refresh', member_id: null, name: null, status: null, flags: [], sound: null })

// ---------- rules (server/src/domain.rs, rules.rs) ----------

const hasSessions = (s: Sub) => s.kind !== 'duration'
const hasDuration = (s: Sub) => s.kind !== 'sessions'
const paidOf = (s: Sub) => db.payments.filter((p) => p.subscription_id === s.id).reduce((a, p) => a + p.amount, 0)
const remaining = (s: Sub) => (s.sessions === null ? null : s.sessions - s.sessions_used)
function valid(s: Sub, d: string) {
  if (d < s.start_date) return false
  if (hasDuration(s) && (!s.end_date || d > s.end_date)) return false
  if (hasSessions(s) && (remaining(s) ?? 0) <= 0) return false
  return true
}
const daysRemaining = (s: Sub, d: string) =>
  s.end_date ? Math.max(0, Math.round((parseDate(s.end_date).getTime() - parseDate(d > s.start_date ? d : s.start_date).getTime()) / 86_400_000) + 1) : null
const debtOf = (s: Sub) => Math.max(0, s.price - paidOf(s))
const subsOf = (id: number) => db.subs.filter((s) => s.member_id === id)

function status(subs: Sub[], d: string) {
  if (!subs.length) return 'none'
  if (!subs.some((s) => valid(s, d))) return 'expired'
  if (subs.some((s) => debtOf(s) > 0)) return 'debt'
  return 'ok'
}

function subView(s: Sub) {
  const d = today()
  const paid = paidOf(s)
  return {
    ...s,
    sessions_remaining: remaining(s),
    days_remaining: daysRemaining(s, d),
    paid,
    debt: Math.max(0, s.price - paid),
    valid: valid(s, d),
    upcoming: s.start_date > d,
  }
}

function current(subs: Sub[]) {
  const d = today()
  const v = subs
    .filter((s) => valid(s, d))
    .sort((a, b) => (a.end_date ?? '9999') .localeCompare(b.end_date ?? '9999') || (remaining(a) ?? 1e9) - (remaining(b) ?? 1e9))[0]
  return v ? { plan_name: v.plan_name, sessions_remaining: remaining(v), days_remaining: daysRemaining(v, d) } : null
}

function row(m: Member) {
  const subs = subsOf(m.id)
  return { ...m, status: status(subs, today()), debt: subs.reduce((a, s) => a + debtOf(s), 0), current: current(subs) }
}

function detail(id: number) {
  const m = db.members.find((x) => x.id === id)
  if (!m) throw new ApiError(404, 'not_found', 'عضو پیدا نشد')
  const subs = subsOf(id).sort((a, b) => b.start_date.localeCompare(a.start_date) || b.id - a.id)
  const payments = db.payments
    .filter((p) => subs.some((s) => s.id === p.subscription_id))
    .map((p) => ({ ...p, plan_name: subs.find((s) => s.id === p.subscription_id)!.plan_name }))
    .sort((a, b) => b.paid_at.localeCompare(a.paid_at))
  return { ...row(m), subscriptions: subs.map(subView), payments }
}

function shiftFlag(at: string, gender: string) {
  if (!db.shifts.length) return null
  const d = at.slice(0, 10)
  const t = at.slice(11, 16)
  const cur = db.shifts.filter((s) => s.weekday === weekday(d) && s.start_time <= t && t < s.end_time)
  if (!cur.length) return 'outside_shift'
  return cur.some((s) => s.gender === gender) ? null : 'wrong_shift'
}

function enter(memberId: number, at: string, source: Visit['entry_source'], snapshot: string | null, faceEvent?: number): LiveEvent {
  const m = db.members.find((x) => x.id === memberId)
  if (!m) throw new ApiError(404, 'not_found', 'عضو پیدا نشد')
  if (m.archived) throw new ApiError(400, 'bad_request', 'این عضو بایگانی شده است')
  if (db.visits.some((v) => v.member_id === memberId && !v.exited_at)) throw new ApiError(409, 'conflict', `${m.full_name} الان داخل باشگاه است`)
  const d = at.slice(0, 10)
  const subs = subsOf(memberId)
  const st = status(subs, d)
  const todays = db.visits.filter((v) => v.member_id === memberId && v.entered_at.startsWith(d))
  const oldest = (pred: (s: Sub) => boolean) => subs.filter((s) => valid(s, d) && pred(s)).sort((a, b) => a.start_date.localeCompare(b.start_date) || a.id - b.id)[0]
  const deduct = todays.some((v) => v.subscription_id) ? undefined : oldest(hasSessions)
  const governing = deduct ?? oldest(() => true)
  const flags: string[] = []
  const gap = db.settings.second_visit_hours * 3_600_000
  if (todays.some((v) => parseDate(at).getTime() - parseDate(v.entered_at).getTime() >= gap)) flags.push('second_visit_today')
  if (governing?.frequency === 'alternate' && db.visits.some((v) => v.member_id === memberId && v.entered_at.startsWith(addDays(d, -1)))) flags.push('alternate_day')
  const sf = shiftFlag(at, m.gender)
  if (sf) flags.push(sf)
  const sound = flags.includes('wrong_shift') && db.settings.wrong_shift_alarm ? 'wrong-shift' : st !== 'ok' ? 'end-of-tuition' : 'welcome'
  const visit: Visit = { id: db.seq++, member_id: memberId, entered_at: at, exited_at: null, entry_source: source, exit_source: null, subscription_id: deduct?.id ?? null, status: st as Visit['status'], flags, snapshot }
  db.visits.push(visit)
  if (deduct) deduct.sessions_used++
  if (faceEvent) db.faceEvents.find((f) => f.id === faceEvent)!.resolved_visit_id = visit.id
  return { kind: 'entry', member_id: memberId, name: m.full_name, status: st, flags, sound }
}

function exit(memberId: number, at: string, source: 'camera' | 'manual'): LiveEvent {
  const m = db.members.find((x) => x.id === memberId)!
  const v = db.visits.find((x) => x.member_id === memberId && !x.exited_at)
  if (!v) throw new ApiError(409, 'conflict', `${m.full_name} داخل باشگاه نیست`)
  v.exited_at = at
  v.exit_source = source
  return { kind: 'exit', member_id: memberId, name: m.full_name, status: null, flags: [], sound: source === 'camera' ? 'goodbye' : null }
}

function visitRow(v: Visit) {
  const m = db.members.find((x) => x.id === v.member_id)!
  return { ...v, full_name: m.full_name, gender: m.gender, current: current(subsOf(m.id)) }
}
const byActivity = (a: Visit, b: Visit) => (b.exited_at ?? b.entered_at).localeCompare(a.exited_at ?? a.entered_at) || b.id - a.id

// a few people already came in today
;(() => {
  const n = new Date()
  const present = db.members.filter((m) => m.face_enrolled && (n.getHours() >= 15 ? m.gender === 'male' : m.gender === 'female')).slice(0, 6)
  present.forEach((m, i) => {
    const t = isoDateTime(new Date(Date.now() - (95 - i * 14) * 60_000))
    if (t.startsWith(today())) {
      try {
        enter(m.id, t, 'camera', `m${m.id}`)
        if (i < 2) exit(m.id, isoDateTime(new Date(parseDate(t).getTime() + 70 * 60_000)), 'camera')
      } catch {
        /* already inside */
      }
    }
  })
})()

// ---------- demo controls (the panel at the bottom of the page) ----------

export const demo = {
  /** someone not inside walks past the camera */
  cameraEntry() {
    const inside = new Set(db.visits.filter((v) => !v.exited_at).map((v) => v.member_id))
    const out = db.members.filter((m) => !m.archived && m.face_enrolled && !inside.has(m.id))
    const m = out[Math.floor(Math.random() * out.length)]
    if (!m) return
    emit(enter(m.id, now(), 'camera', `m${m.id}`))
  },
  cameraExit() {
    const open = db.visits.filter((v) => !v.exited_at)
    const v = open[Math.floor(Math.random() * open.length)]
    if (!v) return
    emit(exit(v.member_id, now(), 'camera'))
  },
  unknownFace() {
    db.faceEvents.push({ id: db.seq++, type: 'unknown', candidates: [], snapshot: `u${db.seq}`, at: now(), resolved_visit_id: null, dismissed: false })
    emit({ ...refresh(), kind: 'face' })
  },
}

// ---------- reports (server/src/reports.rs) ----------

type Col = { key: string; label: string; kind: string }
const col = (key: string, label: string, kind: string): Col => ({ key, label, kind })
const fa = (s: string | number) => String(s).replace(/\d/g, (d) => '۰۱۲۳۴۵۶۷۸۹'[Number(d)])
const jalali = new Intl.DateTimeFormat('fa-IR-u-ca-persian', { year: 'numeric', month: '2-digit', day: '2-digit' })
const jalaliMonth = new Intl.DateTimeFormat('fa-IR-u-ca-persian', { year: 'numeric', month: 'long' })
const DAYS = ['شنبه', 'یکشنبه', 'دوشنبه', 'سه‌شنبه', 'چهارشنبه', 'پنجشنبه', 'جمعه']
const active = () => db.members.filter((m) => !m.archived)
const lastVisit = (id: number) => db.visits.filter((v) => v.member_id === id).map((v) => v.entered_at.slice(0, 10)).sort().pop() ?? null

function report(name: string, q: URLSearchParams) {
  const t = today()
  const g = q.get('gender')
  const gOk = (m: Member) => !g || m.gender === g
  const member = (m: Member) => [m.id, m.full_name]
  switch (name) {
    case 'revenue': {
      const from = q.get('from') ?? t
      const to = q.get('to') ?? t
      const rows = db.payments
        .map((p) => ({ p, s: db.subs.find((s) => s.id === p.subscription_id)! }))
        .map((x) => ({ ...x, m: db.members.find((m) => m.id === x.s.member_id)! }))
        .filter(({ p, s, m }) => p.paid_at.slice(0, 10) >= from && p.paid_at.slice(0, 10) <= to && gOk(m) && (!q.get('plan') || String(s.plan_id) === q.get('plan')) && (!q.get('kind') || s.kind === q.get('kind')))
        .sort((a, b) => a.p.paid_at.localeCompare(b.p.paid_at))
      const total = rows.reduce((a, r) => a + r.p.amount, 0)
      const totals = [
        { label: 'تعداد پرداخت', value: rows.length, kind: 'int' },
        { label: 'جمع درآمد', value: total, kind: 'money' },
      ]
      const group = q.get('group') ?? 'none'
      if (group === 'none') {
        return {
          title: 'درآمد',
          columns: [col('paid_at', 'تاریخ پرداخت', 'datetime'), col('member', 'عضو', 'member'), col('phone', 'موبایل', 'phone'), col('plan', 'تعرفه', 'text'), col('price', 'مبلغ اشتراک', 'money'), col('amount', 'پرداختی', 'money'), col('note', 'یادداشت', 'text')],
          rows: rows.map(({ p, s, m }) => [p.paid_at, member(m), m.phone, s.plan_name, s.price, p.amount, p.note]),
          totals,
        }
      }
      const groups = new Map<string, [string, number, number]>()
      for (const { p } of rows) {
        const d = p.paid_at.slice(0, 10)
        const sat = addDays(d, -weekday(d))
        const [key, label] =
          group === 'week' ? [sat, `هفته‌ی ${jalali.format(parseDate(sat))}`] : group === 'month' ? [jalaliMonth.format(parseDate(d)), jalaliMonth.format(parseDate(d))] : [d, jalali.format(parseDate(d))]
        const e = groups.get(key) ?? [label, 0, 0]
        groups.set(key, [e[0], e[1] + 1, e[2] + p.amount])
      }
      return { title: 'درآمد', columns: [col('period', 'دوره', 'text'), col('count', 'تعداد پرداخت', 'int'), col('amount', 'درآمد', 'money')], rows: [...groups.values()], totals }
    }
    case 'visits': {
      const from = q.get('from') ?? t
      const to = q.get('to') ?? t
      const rows = db.visits
        .filter((v) => {
          const m = db.members.find((x) => x.id === v.member_id)!
          const d = v.entered_at.slice(0, 10)
          return d >= from && d <= to && gOk(m) && (!q.get('status') || v.status === q.get('status')) && (!q.get('flag') || v.flags.includes(q.get('flag')!)) && (!q.get('source') || v.entry_source === q.get('source'))
        })
        .sort((a, b) => b.entered_at.localeCompare(a.entered_at))
      const src = { camera: 'دوربین', manual: 'دستی', picked: 'انتخاب از دوربین' }
      return {
        title: 'ورودها',
        columns: [col('member', 'عضو', 'member'), col('phone', 'موبایل', 'phone'), col('entered', 'ورود', 'datetime'), col('exited', 'خروج', 'time'), col('minutes', 'مدت حضور', 'minutes'), col('status', 'شهریه', 'status'), col('flags', 'برچسب‌ها', 'flags'), col('source', 'منبع', 'text')],
        rows: rows.map((v) => {
          const m = db.members.find((x) => x.id === v.member_id)!
          const mins = v.exited_at ? Math.round((parseDate(v.exited_at).getTime() - parseDate(v.entered_at).getTime()) / 60_000) : null
          return [member(m), m.phone, v.entered_at, v.exited_at, mins, v.status, v.flags, src[v.entry_source]]
        }),
        totals: [
          { label: 'تعداد ورود', value: rows.length, kind: 'int' },
          { label: 'تعداد افراد', value: new Set(rows.map((v) => v.member_id)).size, kind: 'int' },
        ],
      }
    }
    case 'expiring': {
      const maxS = Number(q.get('sessions') ?? 3)
      const maxD = Number(q.get('days') ?? 7)
      const rows: unknown[][] = []
      for (const m of active().filter(gOk)) {
        const subs = subsOf(m.id)
        if (subs.some((s) => s.start_date > t)) continue
        for (const s of subs.filter((s) => valid(s, t) && (!q.get('plan') || String(s.plan_id) === q.get('plan')))) {
          const r = remaining(s)
          const dr = daysRemaining(s, t)
          if ((r !== null && r <= maxS) || (dr !== null && dr <= maxD)) rows.push([member(m), m.phone, s.plan_name, r, dr, s.end_date])
        }
      }
      return { title: 'در حال اتمام', columns: [col('member', 'عضو', 'member'), col('phone', 'موبایل', 'phone'), col('plan', 'تعرفه', 'text'), col('sessions', 'جلسات باقی‌مانده', 'int'), col('days', 'روزهای باقی‌مانده', 'int'), col('end', 'تاریخ پایان', 'date')], rows, totals: [{ label: 'تعداد', value: rows.length, kind: 'int' }] }
    }
    case 'expired': {
      const minDays = Number(q.get('min_days') ?? 0)
      const rows: [string, unknown[]][] = []
      for (const m of active().filter(gOk)) {
        const subs = subsOf(m.id)
        if (status(subs, t) !== 'expired') continue
        const ended = subs
          .map((s) => {
            const byDate = s.end_date && s.end_date < t ? s.end_date : null
            const bySess = (remaining(s) ?? 1) <= 0 ? (db.visits.filter((v) => v.subscription_id === s.id).map((v) => v.entered_at.slice(0, 10)).sort().pop() ?? s.start_date) : null
            const e = [byDate, bySess].filter(Boolean).sort()[0] as string | undefined
            return e ? ([e, s] as const) : null
          })
          .filter((x) => x !== null)
          .sort((a, b) => b[0].localeCompare(a[0]))[0]
        if (!ended) continue
        const since = Math.round((parseDate(t).getTime() - parseDate(ended[0]).getTime()) / 86_400_000)
        if (since < minDays || (q.get('from') && ended[0] < q.get('from')!) || (q.get('to') && ended[0] > q.get('to')!)) continue
        rows.push([ended[0], [member(m), m.phone, ended[1].plan_name, ended[0], since, lastVisit(m.id)]])
      }
      rows.sort((a, b) => b[0].localeCompare(a[0]))
      return { title: 'تمام‌شده‌ها', columns: [col('member', 'عضو', 'member'), col('phone', 'موبایل', 'phone'), col('plan', 'آخرین تعرفه', 'text'), col('ended', 'تاریخ اتمام', 'date'), col('since', 'روز از اتمام', 'int'), col('last_visit', 'آخرین ورود', 'date')], rows: rows.map((r) => r[1]), totals: [{ label: 'تعداد', value: rows.length, kind: 'int' }] }
    }
    case 'debtors': {
      const min = Math.max(1, Number(q.get('min_debt') ?? 1))
      const rows = active()
        .filter(gOk)
        .flatMap((m) => subsOf(m.id).filter((s) => debtOf(s) >= min).map((s) => [member(m), m.phone, s.plan_name, s.start_date, s.price, paidOf(s), debtOf(s)]))
        .sort((a, b) => (b[6] as number) - (a[6] as number))
      return {
        title: 'بدهکاران',
        columns: [col('member', 'عضو', 'member'), col('phone', 'موبایل', 'phone'), col('plan', 'اشتراک', 'text'), col('start', 'شروع', 'date'), col('price', 'مبلغ', 'money'), col('paid', 'پرداختی', 'money'), col('debt', 'بدهی', 'money')],
        rows,
        totals: [
          { label: 'تعداد', value: rows.length, kind: 'int' },
          { label: 'جمع بدهی', value: rows.reduce((a, r) => a + (r[6] as number), 0), kind: 'money' },
        ],
      }
    }
    case 'absent': {
      const minDays = Number(q.get('days') ?? 7)
      const rows = active()
        .filter(gOk)
        .flatMap((m) => {
          const v = subsOf(m.id).filter((s) => valid(s, t))
          if (!v.length) return []
          const start = v.map((s) => s.start_date).sort()[0]
          const last = lastVisit(m.id)
          const since = last && last > start ? last : start
          const days = Math.round((parseDate(t).getTime() - parseDate(since).getTime()) / 86_400_000)
          return days >= minDays ? [[member(m), m.phone, current(subsOf(m.id))?.plan_name ?? '', last, days]] : []
        })
        .sort((a, b) => (b[4] as number) - (a[4] as number))
      return { title: 'غایبین', columns: [col('member', 'عضو', 'member'), col('phone', 'موبایل', 'phone'), col('plan', 'اشتراک فعلی', 'text'), col('last_visit', 'آخرین ورود', 'date'), col('days', 'روز غیبت', 'int')], rows, totals: [{ label: 'تعداد', value: rows.length, kind: 'int' }] }
    }
    case 'busy': {
      const from = q.get('from') ?? addDays(t, -29)
      const to = q.get('to') ?? t
      const counts = Array.from({ length: 7 }, () => Array<number>(24).fill(0))
      let total = 0
      for (const v of db.visits) {
        const d = v.entered_at.slice(0, 10)
        const m = db.members.find((x) => x.id === v.member_id)!
        if (d < from || d > to || !gOk(m)) continue
        counts[weekday(d)][Number(v.entered_at.slice(11, 13))]++
        total++
      }
      const occ = Array<number>(7).fill(0)
      for (let d = from; d <= to; d = addDays(d, 1)) occ[weekday(d)]++
      const used = [...Array(24).keys()].filter((h) => counts.some((r) => r[h] > 0))
      const first = Math.min(6, used[0] ?? 6)
      const last = Math.max(22, used[used.length - 1] ?? 22)
      const hours = [...Array(last - first + 1).keys()].map((i) => first + i)
      return {
        title: 'ساعات شلوغی',
        columns: [col('day', 'روز', 'text'), ...hours.map((h) => col(`h${h}`, fa(h), 'heat'))],
        rows: DAYS.map((day, i) => [day, ...hours.map((h) => (occ[i] ? Math.round((counts[i][h] / occ[i]) * 10) / 10 : 0))]),
        totals: [{ label: 'کل ورودها', value: total, kind: 'int' }],
      }
    }
  }
  throw new ApiError(404, 'not_found', 'گزارش پیدا نشد')
}

// ---------- face enrollment (simulated camera) ----------

let enrollment: { member: number; started: number } | null = null
function enrollStatus() {
  if (!enrollment) return { state: 'idle' }
  const collected = Math.min(8, Math.floor((Date.now() - enrollment.started) / 450))
  const hints = ['look', 'turned', 'look', 'too_small', 'look']
  return {
    state: collected >= 8 ? 'ready' : 'collecting',
    member_id: String(enrollment.member),
    collected,
    target: 8,
    hint: '',
    hint_code: collected >= 8 ? 'good' : hints[Math.floor((Date.now() - enrollment.started) / 1200) % hints.length],
  }
}

// ---------- router ----------

export async function handle(method: string, path: string, body: unknown): Promise<unknown> {
  await new Promise((r) => setTimeout(r, 60)) // feel like a network
  const url = new URL(path, 'http://demo')
  const p = url.pathname
  const q = url.searchParams
  const b = (body ?? {}) as Record<string, unknown>
  const id = Number(p.split('/')[2])
  const t = today()
  const M = (re: RegExp, m = 'GET') => method === m && re.test(p)

  if (M(/^\/auth\/state$/)) return { setup_required: false, authenticated: true }
  if (p.startsWith('/auth/')) return { ok: true }
  if (M(/^\/settings$/)) return db.settings
  if (M(/^\/settings$/, 'PUT')) return Object.assign(db.settings, b)

  if (M(/^\/plans$/)) return q.get('all') ? db.plans : db.plans.filter((x) => x.active)
  if (M(/^\/plans$/, 'POST')) {
    const plan = { ...(b as object), id: db.seq++ } as Db['plans'][number]
    db.plans.push(plan)
    return plan
  }
  if (M(/^\/plans\/\d+$/, 'PUT')) return Object.assign(db.plans.find((x) => x.id === id)!, b)

  if (M(/^\/members$/)) {
    const term = (q.get('q') ?? '').replace(/[۰-۹]/g, (d) => String(d.charCodeAt(0) - 0x06f0))
    return db.members
      .filter((m) => m.archived === (q.get('archived') === 'true'))
      .filter((m) => !term || m.full_name.includes(term) || m.phone.includes(term))
      .filter((m) => !q.get('gender') || m.gender === q.get('gender'))
      .map(row)
      .filter((r) => !q.get('status') || (q.get('status') === 'alert' ? r.status !== 'ok' : r.status === q.get('status')))
      .sort((a, b) => b.id - a.id)
  }
  if (M(/^\/members$/, 'POST')) {
    if (db.members.some((m) => m.phone === b.phone)) throw new ApiError(409, 'conflict', 'عضو دیگری با این شماره موبایل ثبت شده است')
    const m: Member = { id: db.seq++, full_name: String(b.full_name), phone: String(b.phone), birth_date: String(b.birth_date), gender: b.gender as Member['gender'], face_enrolled: false, notes: String(b.notes ?? ''), archived: false, created_at: now() }
    db.members.push(m)
    return detail(m.id)
  }
  if (M(/^\/members\/\d+$/)) return detail(id)
  if (M(/^\/members\/\d+$/, 'PUT')) {
    Object.assign(db.members.find((m) => m.id === id)!, b)
    return detail(id)
  }
  if (M(/^\/members\/\d+\/archive$/, 'POST')) {
    const m = db.members.find((x) => x.id === id)!
    m.archived = Boolean(b.archived)
    if (m.archived) m.face_enrolled = false
    return detail(id)
  }
  if (M(/^\/members\/\d+\/subscriptions\/preview$/)) {
    const plan = db.plans.find((x) => x.id === Number(q.get('plan_id')))!
    let start = q.get('start_date') ?? t
    if (!q.get('start_date') && plan.duration_days) {
      const ends = subsOf(id).filter((s) => hasDuration(s) && s.end_date && s.end_date >= t).map((s) => s.end_date!).sort()
      if (ends.length) start = addDays(ends[ends.length - 1], 1)
    }
    return { start_date: start, end_date: plan.duration_days ? addDays(start, plan.duration_days - 1) : null, price: plan.price }
  }
  if (M(/^\/members\/\d+\/subscriptions$/, 'POST')) {
    const plan = db.plans.find((x) => x.id === Number(b.plan_id))!
    const start = String(b.start_date)
    const sub: Sub = { id: db.seq++, member_id: id, plan_id: plan.id, plan_name: plan.name, kind: plan.kind, sessions: plan.sessions, duration_days: plan.duration_days, frequency: plan.frequency, shower: plan.shower, locker: plan.locker, price: Number(b.price), start_date: start, end_date: plan.duration_days ? addDays(start, plan.duration_days - 1) : null, sessions_used: 0, created_at: now() }
    db.subs.push(sub)
    if (Number(b.paid) > 0) db.payments.push({ id: db.seq++, subscription_id: sub.id, amount: Number(b.paid), paid_at: now(), note: String(b.note ?? '') })
    return detail(id)
  }
  if (M(/^\/subscriptions\/\d+\/payments$/, 'POST')) {
    const s = db.subs.find((x) => x.id === id)!
    db.payments.push({ id: db.seq++, subscription_id: id, amount: Number(b.amount), paid_at: now(), note: String(b.note ?? '') })
    return detail(s.member_id)
  }
  if (M(/^\/members\/\d+\/visits$/)) return db.visits.filter((v) => v.member_id === id).sort(byActivity).map(visitRow)
  if (M(/^\/members\/\d+\/enter$/, 'POST')) {
    const fe = b.face_event_id as number | undefined
    const ev = enter(id, now(), fe ? 'picked' : 'manual', fe ? (db.faceEvents.find((f) => f.id === fe)?.snapshot ?? null) : null, fe)
    emit(ev)
    return ev
  }
  if (M(/^\/members\/\d+\/exit$/, 'POST')) {
    exit(id, now(), 'manual')
    emit(refresh())
    return null
  }
  if (M(/^\/visits\/\d+\/cancel$/, 'POST')) {
    const v = db.visits.find((x) => x.id === id)!
    if (v.subscription_id) db.subs.find((s) => s.id === v.subscription_id)!.sessions_used--
    db.faceEvents.forEach((f) => f.resolved_visit_id === id && (f.resolved_visit_id = null))
    db.visits.splice(db.visits.indexOf(v), 1)
    emit(refresh())
    return null
  }
  if (M(/^\/face-events\/\d+\/dismiss$/, 'POST')) {
    db.faceEvents.find((f) => f.id === id)!.dismissed = true
    emit(refresh())
    return null
  }

  if (M(/^\/reception$/)) {
    const visits = db.visits.filter((v) => v.entered_at.startsWith(t) || !v.exited_at).sort(byActivity)
    return {
      inside: db.visits.filter((v) => !v.exited_at).length,
      entries_today: new Set(db.visits.filter((v) => v.entered_at.startsWith(t)).map((v) => v.member_id)).size,
      visits: visits.map(visitRow),
      face_events: db.faceEvents
        .filter((f) => f.at.startsWith(t) && !f.dismissed && !f.resolved_visit_id)
        .reverse()
        .map((f) => ({ ...f, candidates: f.candidates.map((c) => ({ member_id: Number(c.member_id), full_name: db.members.find((m) => m.id === Number(c.member_id))!.full_name, score: c.score })) })),
    }
  }

  if (M(/^\/shifts$/)) return [...db.shifts].sort((a, b) => a.weekday - b.weekday || a.start_time.localeCompare(b.start_time))
  if (M(/^\/shifts\/current$/)) {
    const tm = now().slice(11, 16)
    const day = db.shifts.filter((s) => s.weekday === weekday(t)).sort((a, b) => a.start_time.localeCompare(b.start_time))
    return { uses_shifts: db.shifts.length > 0, current: day.find((s) => s.start_time <= tm && tm < s.end_time) ?? null, next: day.find((s) => s.start_time > tm) ?? null }
  }
  if (M(/^\/shifts$/, 'POST')) {
    for (const d of b.weekdays as number[]) db.shifts.push({ id: db.seq++, weekday: d, start_time: String(b.start_time), end_time: String(b.end_time), gender: b.gender as 'male' })
    return handle('GET', '/shifts', null)
  }
  if (M(/^\/shifts\/\d+$/, 'PUT')) {
    Object.assign(db.shifts.find((s) => s.id === id)!, { weekday: (b.weekdays as number[])[0], start_time: b.start_time, end_time: b.end_time, gender: b.gender })
    return handle('GET', '/shifts', null)
  }
  if (M(/^\/shifts\/\d+$/, 'DELETE')) {
    db.shifts = db.shifts.filter((s) => s.id !== id)
    return null
  }

  if (M(/^\/reports\/\w+$/)) return report(p.split('/')[2], q)
  if (M(/^\/dashboard$/)) {
    const days: [string, number, number][] = []
    for (let d = addDays(t, -29); d <= t; d = addDays(d, 1)) {
      days.push([d, db.payments.filter((x) => x.paid_at.startsWith(d)).reduce((a, x) => a + x.amount, 0), new Set(db.visits.filter((v) => v.entered_at.startsWith(d)).map((v) => v.member_id)).size])
    }
    // first day of the Jalali month
    const monthStart = addDays(t, -(Number(new Intl.DateTimeFormat('en-u-ca-persian', { day: 'numeric' }).format(new Date())) - 1))
    return {
      inside: db.visits.filter((v) => !v.exited_at).length,
      entries_today: days[days.length - 1][2],
      revenue_today: days[days.length - 1][1],
      revenue_month: db.payments.filter((x) => x.paid_at.slice(0, 10) >= monthStart).reduce((a, x) => a + x.amount, 0),
      active_members: active().filter((m) => subsOf(m.id).some((s) => valid(s, t))).length,
      expiring_week: (report('expiring', new URLSearchParams()) as { rows: unknown[] }).rows.length,
      total_debt: active().flatMap((m) => subsOf(m.id)).reduce((a, s) => a + debtOf(s), 0),
      no_face: active().filter((m) => !m.face_enrolled).length,
      days,
      busy: report('busy', new URLSearchParams()),
    }
  }

  if (M(/^\/face\/health$/)) return { reachable: true, camera: { connected: true, name: 'دوربین نمایشی', active: true }, fps: 8, events: true }
  if (M(/^\/face\/enroll$/)) return enrollStatus()
  if (M(/^\/face\/enroll$/, 'DELETE')) {
    enrollment = null
    return null
  }
  if (M(/^\/members\/\d+\/face\/start$/, 'POST')) {
    enrollment = { member: id, started: Date.now() }
    return enrollStatus()
  }
  if (M(/^\/members\/\d+\/face\/commit$/, 'POST')) {
    enrollment = null
    db.members.find((m) => m.id === id)!.face_enrolled = true
    return { member_id: String(id), samples: 8 }
  }
  if (M(/^\/members\/\d+\/face$/, 'DELETE')) {
    db.members.find((m) => m.id === id)!.face_enrolled = false
    return null
  }
  throw new ApiError(404, 'not_found', `در نسخه‌ی نمایشی پشتیبانی نمی‌شود: ${method} ${p}`)
}

