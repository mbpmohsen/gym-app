// Live updates from the server (/api/live, SSE): refresh lists and play the
// announcement. Mounted once in Layout, so sounds play on every page.
//
// Only ONE tab plays sounds: the one holding the "gym-sound" Web Lock. If two
// tabs are open, the second stays silent until the first is closed.

import { useQueryClient } from '@tanstack/react-query'
import { useEffect, useSyncExternalStore } from 'react'
import { toast } from 'sonner'

import { play, type Sound } from '@/lib/audio'
import type { Flag, Status } from '@/lib/types'

type LiveEvent = {
  kind: 'entry' | 'exit' | 'face' | 'refresh'
  member_id: number | null
  name: string | null
  status: Status | null
  flags: Flag[]
  sound: Sound | null
}

let leader = false
let connected = false
const listeners = new Set<() => void>()
const notify = () => listeners.forEach((l) => l())

if ('locks' in navigator) {
  // resolves when we get the lock; the never-settling promise keeps it until the tab closes
  void navigator.locks.request('gym-sound', () => {
    leader = true
    notify()
    return new Promise<never>(() => {})
  })
} else {
  leader = true
}

const subscribe = (fn: () => void) => (listeners.add(fn), () => listeners.delete(fn))
export const useSoundLeader = () => useSyncExternalStore(subscribe, () => leader)
export const useLiveConnected = () => useSyncExternalStore(subscribe, () => connected)

export function useLive() {
  const qc = useQueryClient()
  useEffect(() => {
    const es = new EventSource('/api/live')
    es.onopen = () => ((connected = true), notify())
    es.onerror = () => ((connected = false), notify()) // EventSource retries by itself
    es.onmessage = (m) => {
      const ev = JSON.parse(m.data) as LiveEvent
      void qc.invalidateQueries({ queryKey: ['reception'] })
      void qc.invalidateQueries({ queryKey: ['face-health'] })
      if (ev.member_id) {
        void qc.invalidateQueries({ queryKey: ['member', ev.member_id] })
        void qc.invalidateQueries({ queryKey: ['visits', ev.member_id] })
        void qc.invalidateQueries({ queryKey: ['members'] })
      }
      if (ev.kind === 'entry' && ev.flags.includes('wrong_shift')) toast.error(`${ev.name}: سانس نامعتبر`)
      else if (ev.kind === 'entry' && ev.status !== 'ok') toast.warning(`${ev.name}: پایان شهریه یا بدهی`)
      if (ev.sound && leader) play(ev.sound).catch((e) => console.error('sound failed', e))
    }
    return () => es.close()
  }, [qc])
}
