// API shapes (mirror server/src/plans.rs and members.rs).

export type Kind = 'sessions' | 'duration' | 'combined'
export type Frequency = 'six_days' | 'alternate'
export type Gender = 'male' | 'female'
export type Status = 'none' | 'expired' | 'debt' | 'ok'

export type Plan = {
  id: number
  name: string
  kind: Kind
  sessions: number | null
  duration_days: number | null
  frequency: Frequency
  shower: boolean
  locker: boolean
  price: number
  active: boolean
}

export type CurrentSub = { plan_name: string; sessions_remaining: number | null; days_remaining: number | null }

export type MemberRow = {
  id: number
  full_name: string
  phone: string
  gender: Gender
  birth_date: string
  face_enrolled: boolean
  archived: boolean
  status: Status
  debt: number
  current: CurrentSub | null
  created_at: string
}

export type Subscription = {
  id: number
  plan_name: string
  kind: Kind
  sessions: number | null
  sessions_used: number
  sessions_remaining: number | null
  duration_days: number | null
  start_date: string
  end_date: string | null
  days_remaining: number | null
  frequency: Frequency
  shower: boolean
  locker: boolean
  price: number
  paid: number
  debt: number
  valid: boolean
  upcoming: boolean
  created_at: string
}

export type Payment = { id: number; subscription_id: number; plan_name: string; amount: number; paid_at: string; note: string }

export type MemberDetail = MemberRow & { notes: string; subscriptions: Subscription[]; payments: Payment[] }

export const KIND_LABEL: Record<Kind, string> = { sessions: 'جلسه‌ای', duration: 'زمانی', combined: 'ترکیبی' }
export const FREQ_LABEL: Record<Frequency, string> = { six_days: '۶ روز در هفته', alternate: 'یک روز در میان' }
export const GENDER_LABEL: Record<Gender, string> = { male: 'آقا', female: 'خانم' }

// ---- reception (mirror server/src/visits.rs) ----

export type Flag = 'second_visit_today' | 'alternate_day' | 'wrong_shift' | 'outside_shift'

export type Visit = {
  id: number
  member_id: number
  full_name: string
  gender: Gender
  entered_at: string
  exited_at: string | null
  entry_source: 'camera' | 'manual' | 'picked'
  exit_source: 'camera' | 'manual' | 'auto' | null
  status: Status
  flags: Flag[]
  snapshot: string | null
  current: CurrentSub | null
}

export type FaceEvent = {
  id: number
  type: 'unknown' | 'uncertain'
  candidates: { member_id: number; full_name: string; score: number }[]
  snapshot: string | null
  at: string
}

export type Reception = { inside: number; entries_today: number; visits: Visit[]; face_events: FaceEvent[] }

export const FLAG_LABEL: Record<Flag, { label: string; variant: 'warning' | 'danger' | 'secondary' }> = {
  second_visit_today: { label: 'بار دوم امروز', variant: 'secondary' },
  alternate_day: { label: 'یک روز در میان: دیروز هم آمده', variant: 'warning' },
  wrong_shift: { label: 'سانس نامعتبر', variant: 'danger' },
  outside_shift: { label: 'خارج از سانس', variant: 'warning' },
}
