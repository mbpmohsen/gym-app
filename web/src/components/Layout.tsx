// App shell: sidebar on the right (RTL), header with date / sound / theme, content.

import { useQuery, useQueryClient } from '@tanstack/react-query'
import {
  CameraIcon,
  CameraOffIcon,
  CalendarClockIcon,
  ChartColumnIcon,
  DoorOpenIcon,
  LayoutDashboardIcon,
  LogOutIcon,
  SettingsIcon,
  TagsIcon,
  UsersIcon,
  Volume2Icon,
  VolumeXIcon,
} from 'lucide-react'
import { useEffect, useSyncExternalStore } from 'react'
import { motion } from 'motion/react'
import { NavLink, Outlet, useLocation } from 'react-router'

import { PlateMark } from '@/components/page'
import { ThemeMenu } from '@/components/ThemeMenu'
import { Button } from '@/components/ui/button'
import { get, post } from '@/lib/api'
import { audioReady, loadVoice, onAudioChange, unlockAudio, type Voice } from '@/lib/audio'
import { jalaliDate, weekday } from '@/lib/format'
import { useLive, useSoundLeader } from '@/lib/live'
import { cn } from '@/lib/utils'

const NAV = [
  { to: '/', label: 'پذیرش', icon: DoorOpenIcon, end: true },
  { to: '/members', label: 'اعضا', icon: UsersIcon },
  { to: '/plans', label: 'تعرفه‌ها', icon: TagsIcon },
  { to: '/shifts', label: 'سانس‌ها', icon: CalendarClockIcon },
  { to: '/reports', label: 'گزارش‌ها', icon: ChartColumnIcon },
  { to: '/dashboard', label: 'داشبورد', icon: LayoutDashboardIcon },
  { to: '/settings', label: 'تنظیمات', icon: SettingsIcon },
]

export type Settings = {
  gym_name: string
  voice: Voice
  exit_min_minutes: number
  second_visit_hours: number
  auto_exit_hours: number
  wrong_shift_alarm: boolean
}

export function useSettings() {
  return useQuery({ queryKey: ['settings'], queryFn: () => get<Settings>('/settings') })
}

export function Layout() {
  const qc = useQueryClient()
  const settings = useSettings()
  const voice = settings.data?.voice
  useLive()
  const page = useLocation().pathname

  useEffect(() => {
    if (voice) loadVoice(voice).catch((e) => console.error('voice load failed', e))
  }, [voice])

  async function logout() {
    await post('/auth/logout')
    qc.clear()
    location.assign('/')
  }

  return (
    <div className="flex min-h-dvh">
      <aside className="bg-sidebar text-sidebar-foreground border-sidebar-border sticky top-0 flex h-dvh w-60 shrink-0 flex-col border-e">
        <div className="flex h-16 items-center gap-3 px-5">
          <PlateMark />
          <span className="truncate font-bold">{settings.data?.gym_name || 'مدیریت باشگاه'}</span>
        </div>
        <nav className="flex-1 px-3 py-2">
          <ul className="grid gap-1">
            {NAV.map((n) => (
              <li key={n.to}>
                <NavLink
                  to={n.to}
                  end={n.end}
                  className={({ isActive }) =>
                    cn(
                      'flex h-10 items-center gap-3 rounded-lg px-3 text-sm font-medium transition-colors',
                      isActive ? 'bg-primary text-primary-foreground shadow-sm' : 'text-muted-foreground hover:bg-accent hover:text-accent-foreground',
                    )
                  }
                >
                  <n.icon className="size-[18px]" />
                  {n.label}
                </NavLink>
              </li>
            ))}
          </ul>
        </nav>
        <div className="p-3">
          <Button variant="ghost" className="text-muted-foreground w-full justify-start" onClick={logout}>
            <LogOutIcon />
            خروج از برنامه
          </Button>
        </div>
      </aside>

      <div className="flex min-w-0 flex-1 flex-col">
        <header className="bg-background/80 sticky top-0 z-10 flex h-16 items-center justify-between gap-4 border-b px-8 backdrop-blur">
          <p className="text-muted-foreground text-sm">
            <span className="text-foreground font-medium">{weekday(new Date())}</span> {jalaliDate(new Date())}
          </p>
          <div className="flex items-center gap-1">
            <FaceStatus />
            <SoundStatus />
            <ThemeMenu />
          </div>
        </header>
        <main className="mx-auto w-full max-w-6xl flex-1 px-8 py-8">
          <motion.div key={page} initial={{ opacity: 0, y: 6 }} animate={{ opacity: 1, y: 0 }} transition={{ duration: 0.2, ease: 'easeOut' }}>
            <Outlet />
          </motion.div>
        </main>
      </div>
    </div>
  )
}

/** Sound on/off. Browsers block sound until the first click after a reload; any click enables it. */
function SoundStatus() {
  const ready = useSyncExternalStore(onAudioChange, audioReady)
  const leader = useSoundLeader()

  useEffect(() => {
    if (ready) return
    const unlock = () => void unlockAudio()
    window.addEventListener('pointerdown', unlock)
    window.addEventListener('keydown', unlock)
    return () => {
      window.removeEventListener('pointerdown', unlock)
      window.removeEventListener('keydown', unlock)
    }
  }, [ready])

  if (!leader) {
    return (
      <span className="text-muted-foreground flex items-center gap-1.5 px-2 text-sm" title="صدا در زبانه‌ی دیگری از برنامه پخش می‌شود">
        <VolumeXIcon className="size-4" />
        صدا در زبانه‌ی دیگر
      </span>
    )
  }
  if (ready) {
    return (
      <span className="text-muted-foreground flex items-center gap-1.5 px-2 text-sm" title="صدای اعلام فعال است">
        <Volume2Icon className="size-4" />
      </span>
    )
  }
  return (
    <Button variant="outline" size="sm" className="border-warning bg-warning/15 hover:bg-warning/25" onClick={() => void unlockAudio()}>
      <VolumeXIcon />
      صدا خاموش است؛ برای روشن شدن کلیک کنید
    </Button>
  )
}

type FaceHealth = { reachable: boolean; error?: string; camera?: { connected: boolean; name: string }; fps?: number; events?: boolean }

/** Face-service and camera state. Without them nobody is recognized at the door. */
function FaceStatus() {
  const q = useQuery({ queryKey: ['face-health'], queryFn: () => get<FaceHealth>('/face/health'), refetchInterval: 10_000 })
  const h = q.data
  if (!h) return null
  const ok = h.reachable && h.camera?.connected && h.events !== false
  const text = !h.reachable
    ? 'سرویس تشخیص چهره اجرا نیست'
    : !h.camera?.connected
      ? 'دوربین وصل نیست'
      : h.events === false
        ? 'رویدادهای دوربین دریافت نمی‌شود'
        : `دوربین فعال: ${h.camera.name}`
  return (
    <span
      title={text}
      className={cn('flex items-center gap-1.5 rounded-md px-2 py-1 text-sm', ok ? 'text-muted-foreground' : 'bg-destructive/10 text-destructive font-medium')}
    >
      {ok ? <CameraIcon className="size-4" /> : <CameraOffIcon className="size-4" />}
      {!ok && text}
    </span>
  )
}
