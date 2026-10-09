// App-level layout helpers built on the shadcn primitives.

import type { ReactNode } from 'react'

import { Label } from '@/components/ui/label'
import { cn } from '@/lib/utils'

export function PageHeader({ title, description, children }: { title: string; description?: string; children?: ReactNode }) {
  return (
    <header className="mb-8 flex flex-wrap items-end justify-between gap-4">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">{title}</h1>
        {description && <p className="text-muted-foreground mt-1 text-sm">{description}</p>}
      </div>
      {children}
    </header>
  )
}

/** Label + control + hint/error, consistently spaced. */
export function Field({
  label,
  htmlFor,
  hint,
  error,
  className,
  children,
}: {
  label: string
  htmlFor?: string
  hint?: ReactNode
  error?: string
  className?: string
  children: ReactNode
}) {
  return (
    <div className={cn('grid content-start gap-2', className)}>
      <Label htmlFor={htmlFor}>{label}</Label>
      {children}
      {error ? <p className="text-destructive text-sm">{error}</p> : hint ? <p className="text-muted-foreground text-sm">{hint}</p> : null}
    </div>
  )
}

/** The app mark: a bumper plate seen from the front, in the theme's accent. */
export function PlateMark({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 32 32" aria-hidden className={cn('size-8', className)}>
      <circle cx="16" cy="16" r="15" fill="var(--primary)" />
      <circle cx="16" cy="16" r="9.5" fill="var(--foreground)" opacity="0.9" />
      <circle cx="16" cy="16" r="3" fill="var(--background)" />
    </svg>
  )
}
