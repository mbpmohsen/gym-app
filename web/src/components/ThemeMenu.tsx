import { MonitorIcon, MoonIcon, SunIcon } from 'lucide-react'

import { Button } from '@/components/ui/button'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { ACCENTS, setTheme, useTheme, type Accent, type Mode } from '@/lib/theme'

export const MODES: { id: Mode; label: string; icon: typeof SunIcon }[] = [
  { id: 'light', label: 'روشن', icon: SunIcon },
  { id: 'dark', label: 'تیره', icon: MoonIcon },
  { id: 'system', label: 'مطابق سیستم', icon: MonitorIcon },
]

/** Header button: quick theme switching. The full picker lives in Settings. */
export function ThemeMenu() {
  const t = useTheme()
  const Icon = MODES.find((m) => m.id === t.mode)!.icon
  return (
    <DropdownMenu dir="rtl">
      <DropdownMenuTrigger asChild>
        <Button variant="ghost" size="icon" aria-label="تم">
          <Icon />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-44">
        <DropdownMenuLabel>حالت</DropdownMenuLabel>
        <DropdownMenuRadioGroup value={t.mode} onValueChange={(v) => setTheme({ mode: v as Mode })}>
          {MODES.map((m) => (
            <DropdownMenuRadioItem key={m.id} value={m.id}>
              <m.icon />
              {m.label}
            </DropdownMenuRadioItem>
          ))}
        </DropdownMenuRadioGroup>
        <DropdownMenuSeparator />
        <DropdownMenuLabel>رنگ</DropdownMenuLabel>
        <DropdownMenuRadioGroup value={t.accent} onValueChange={(v) => setTheme({ accent: v as Accent })}>
          {ACCENTS.map((a) => (
            <DropdownMenuRadioItem key={a.id} value={a.id}>
              <span className="size-3.5 rounded-full" style={{ background: a.swatch }} />
              {a.label}
            </DropdownMenuRadioItem>
          ))}
        </DropdownMenuRadioGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
