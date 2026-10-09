// Demo build only: simulate what the camera at the door would do.

import { CodeIcon, DoorClosedIcon, DoorOpenIcon, ScanFaceIcon } from 'lucide-react'

import { Button } from '@/components/ui/button'

export function DemoBar() {
  const run = (fn: 'cameraEntry' | 'cameraExit' | 'unknownFace') => void import('@/demo/server').then((d) => d.demo[fn]())
  return (
    <div className="bg-card/95 fixed inset-x-0 bottom-4 z-40 mx-auto flex w-fit max-w-[calc(100%-2rem)] flex-wrap items-center gap-2 rounded-xl border p-2 shadow-lg backdrop-blur">
      <span className="text-muted-foreground px-2 text-xs leading-tight">
        نسخه‌ی نمایشی
        <br />
        شبیه‌سازی دوربین:
      </span>
      <Button size="sm" onClick={() => run('cameraEntry')}>
        <DoorOpenIcon />
        ورود یک عضو
      </Button>
      <Button size="sm" variant="secondary" onClick={() => run('cameraExit')}>
        <DoorClosedIcon />
        خروج یک عضو
      </Button>
      <Button size="sm" variant="secondary" onClick={() => run('unknownFace')}>
        <ScanFaceIcon />
        چهره‌ی ناشناس
      </Button>
      <Button size="sm" variant="ghost" asChild>
        <a href="https://github.com/mbpmohsen/gym-app" target="_blank" rel="noreferrer">
          <CodeIcon />
          کد منبع
        </a>
      </Button>
    </div>
  )
}
