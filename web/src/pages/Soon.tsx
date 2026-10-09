import { HammerIcon } from 'lucide-react'

import { PageHeader } from '@/components/page'
import { num } from '@/lib/format'

/** Placeholder for pages of later milestones (see docs/SPEC.md §7). */
export function Soon({ title, what, milestone }: { title: string; what: string; milestone: number }) {
  return (
    <>
      <PageHeader title={title} />
      <div className="text-muted-foreground grid place-items-center gap-3 rounded-xl border border-dashed py-20 text-center">
        <HammerIcon className="size-8 opacity-60" />
        <p>
          {what}
          <br />
          در مرحله‌ی {num(milestone)} ساخته می‌شود.
        </p>
      </div>
    </>
  )
}
