// Demo data: an in-memory gym with fictional members, generated fresh on every
// page load (nothing is stored; a reload resets the demo).

export type Gender = 'male' | 'female'
export type Member = {
  id: number
  full_name: string
  phone: string
  birth_date: string
  gender: Gender
  face_enrolled: boolean
  notes: string
  archived: boolean
  created_at: string
}
export type Plan = {
  id: number
  name: string
  kind: 'sessions' | 'duration' | 'combined'
  sessions: number | null
  duration_days: number | null
  frequency: 'six_days' | 'alternate'
  shower: boolean
  locker: boolean
  price: number
  active: boolean
}
export type Sub = {
  id: number
  member_id: number
  plan_id: number | null
  plan_name: string
  kind: Plan['kind']
  sessions: number | null
  duration_days: number | null
  frequency: Plan['frequency']
  shower: boolean
  locker: boolean
  price: number
  start_date: string
  end_date: string | null
  sessions_used: number
  created_at: string
}
export type Payment = { id: number; subscription_id: number; amount: number; paid_at: string; note: string }
export type Visit = {
  id: number
  member_id: number
  entered_at: string
  exited_at: string | null
  entry_source: 'camera' | 'manual' | 'picked'
  exit_source: 'camera' | 'manual' | 'auto' | null
  subscription_id: number | null
  status: 'ok' | 'expired' | 'debt' | 'none'
  flags: string[]
  snapshot: string | null
}
export type FaceEvent = {
  id: number
  type: 'unknown' | 'uncertain'
  candidates: { member_id: string; score: number }[]
  snapshot: string | null
  at: string
  resolved_visit_id: number | null
  dismissed: boolean
}
export type Shift = { id: number; weekday: number; start_time: string; end_time: string; gender: Gender }
export type Settings = {
  gym_name: string
  voice: Gender
  exit_min_minutes: number
  second_visit_hours: number
  auto_exit_hours: number
  wrong_shift_alarm: boolean
}

export type Db = {
  members: Member[]
  plans: Plan[]
  subs: Sub[]
  payments: Payment[]
  visits: Visit[]
  faceEvents: FaceEvent[]
  shifts: Shift[]
  settings: Settings
  seq: number
}

// ---------- dates (local time, ISO strings like the real server) ----------

const pad = (n: number) => String(n).padStart(2, '0')
export const isoDate = (d: Date) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`
export const isoDateTime = (d: Date) => `${isoDate(d)} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`
export const parseDate = (s: string) => {
  const [d, t = '00:00:00'] = s.split(' ')
  const [y, m, day] = d.split('-').map(Number)
  const [hh, mm, ss] = t.split(':').map(Number)
  return new Date(y, m - 1, day, hh, mm, ss || 0)
}
export const addDays = (iso: string, n: number) => {
  const d = parseDate(iso)
  d.setDate(d.getDate() + n)
  return isoDate(d)
}
export const today = () => isoDate(new Date())
export const now = () => isoDateTime(new Date())
/** Iranian week: Saturday = 0 */
export const weekday = (iso: string) => (parseDate(iso).getDay() + 1) % 7

// ---------- seed ----------

const MALE = ['علی', 'رضا', 'محمد', 'حسین', 'امیر', 'مهدی', 'پویا', 'نیما', 'کاوه', 'سینا', 'آرش', 'بهراد', 'سامان', 'میلاد']
const FEMALE = ['سارا', 'مریم', 'نگار', 'الهام', 'لیلا', 'شیما', 'بهار', 'نازنین', 'ترانه', 'پریسا', 'یاسمن', 'مهسا']
const LAST = ['رضایی', 'احمدی', 'کریمی', 'حسینی', 'موسوی', 'جعفری', 'کاظمی', 'نوری', 'صادقی', 'رحیمی', 'محمدی', 'تهرانی', 'شریفی', 'نیک‌نام']

/** Small deterministic PRNG so every visitor sees the same demo gym. */
function rng(seed: number) {
  return () => {
    seed = (seed * 1664525 + 1013904223) % 4294967296
    return seed / 4294967296
  }
}

export function seed(): Db {
  const r = rng(1405)
  const pick = <T,>(a: T[]) => a[Math.floor(r() * a.length)]
  const t = today()
  const db: Db = {
    members: [],
    plans: [
      { id: 1, name: '۱۲ جلسه', kind: 'sessions', sessions: 12, duration_days: null, frequency: 'six_days', shower: false, locker: false, price: 1_200_000, active: true },
      { id: 2, name: 'یک ماهه با دوش', kind: 'duration', sessions: null, duration_days: 30, frequency: 'six_days', shower: true, locker: false, price: 1_800_000, active: true },
      { id: 3, name: 'سه ماهه یک روز در میان', kind: 'combined', sessions: 36, duration_days: 90, frequency: 'alternate', shower: false, locker: true, price: 3_900_000, active: true },
      { id: 4, name: '۸ جلسه', kind: 'sessions', sessions: 8, duration_days: null, frequency: 'six_days', shower: false, locker: false, price: 850_000, active: true },
    ],
    subs: [],
    payments: [],
    visits: [],
    faceEvents: [],
    shifts: [],
    settings: { gym_name: 'باشگاه نمونه', voice: 'male', exit_min_minutes: 5, second_visit_hours: 3, auto_exit_hours: 4, wrong_shift_alarm: true },
    seq: 1000,
  }
  // women: Sat–Wed mornings; men: every afternoon/evening
  let sid = 1
  for (const d of [0, 1, 2, 3, 4]) db.shifts.push({ id: sid++, weekday: d, start_time: '08:00', end_time: '14:00', gender: 'female' })
  for (const d of [0, 1, 2, 3, 4, 5, 6]) db.shifts.push({ id: sid++, weekday: d, start_time: '15:00', end_time: '23:00', gender: 'male' })

  for (let id = 1; id <= 38; id++) {
    const gender: Gender = r() < 0.55 ? 'male' : 'female'
    const joined = addDays(t, -Math.floor(20 + r() * 300))
    db.members.push({
      id,
      full_name: `${pick(gender === 'male' ? MALE : FEMALE)} ${pick(LAST)}`,
      phone: `0912${String(3000000 + id * 7919).padStart(7, '0')}`,
      birth_date: `${1975 + Math.floor(r() * 30)}-${pad(1 + Math.floor(r() * 12))}-${pad(1 + Math.floor(r() * 28))}`,
      gender,
      face_enrolled: r() > 0.08,
      notes: '',
      archived: false,
      created_at: `${joined} 10:00:00`,
    })
    const plan = pick([db.plans[0], db.plans[0], db.plans[1], db.plans[2], db.plans[3]])
    const start = addDays(t, -Math.floor(r() * (plan.duration_days ?? 45)))
    const sub: Sub = {
      id,
      member_id: id,
      plan_id: plan.id,
      plan_name: plan.name,
      kind: plan.kind,
      sessions: plan.sessions,
      duration_days: plan.duration_days,
      frequency: plan.frequency,
      shower: plan.shower,
      locker: plan.locker,
      price: plan.price,
      start_date: start,
      end_date: plan.duration_days ? addDays(start, plan.duration_days - 1) : null,
      sessions_used: 0,
      created_at: `${start} 09:30:00`,
    }
    db.subs.push(sub)
    const paid = r() < 0.75 ? plan.price : Math.round((plan.price * (0.3 + r() * 0.4)) / 10000) * 10000
    db.payments.push({ id, subscription_id: id, amount: paid, paid_at: `${start} ${pad(9 + Math.floor(r() * 12))}:${pad(Math.floor(r() * 60))}:00`, note: '' })

    // past visits, respecting the member's shift
    for (let d = start; d < t; d = addDays(d, 1)) {
      const wd = weekday(d)
      if (gender === 'female' && wd > 4) continue
      if (r() > (plan.frequency === 'alternate' ? 0.4 : 0.55)) continue
      if (sub.sessions !== null && sub.sessions_used >= sub.sessions) break
      const hour = gender === 'female' ? 8 + Math.floor(r() * 5) : pick([15, 16, 17, 18, 18, 19, 19, 19, 20, 20, 21, 22])
      const enter = `${d} ${pad(hour)}:${pad(Math.floor(r() * 60))}:00`
      const exit = isoDateTime(new Date(parseDate(enter).getTime() + (55 + r() * 70) * 60_000))
      db.visits.push({
        id: db.seq++,
        member_id: id,
        entered_at: enter,
        exited_at: exit,
        entry_source: r() < 0.9 ? 'camera' : 'manual',
        exit_source: 'camera',
        subscription_id: sub.sessions !== null ? sub.id : null,
        status: 'ok',
        flags: [],
        snapshot: `m${id}`,
      })
      if (sub.sessions !== null) sub.sessions_used++
    }
  }
  // two members already expired, one without any subscription
  db.subs.find((s) => s.member_id === 5)!.sessions_used = db.subs.find((s) => s.member_id === 5)!.sessions ?? 0
  db.subs = db.subs.filter((s) => s.member_id !== 9)
  db.payments = db.payments.filter((p) => p.subscription_id !== 9)

  // a pending "not sure" card at the door
  db.faceEvents.push({
    id: db.seq++,
    type: 'uncertain',
    candidates: [
      { member_id: '3', score: 0.36 },
      { member_id: '11', score: 0.33 },
    ],
    snapshot: 'u1',
    at: isoDateTime(new Date(Date.now() - 6 * 60_000)),
    resolved_visit_id: null,
    dismissed: false,
  })
  return db
}
