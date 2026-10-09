// Search-and-pick a member in a dialog (manual entry, resolving an unknown face).

import { useQuery } from '@tanstack/react-query'
import { SearchIcon } from 'lucide-react'
import { useState } from 'react'

import { StatusBadge } from '@/components/StatusBadge'
import { DialogContent, DialogDescription, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { get } from '@/lib/api'
import { faDigits } from '@/lib/format'
import { GENDER_LABEL, type MemberRow } from '@/lib/types'

export function MemberPicker({
  title,
  description,
  busy,
  onPick,
}: {
  title: string
  description?: string
  busy?: boolean
  onPick: (m: MemberRow) => void
}) {
  const [q, setQ] = useState('')
  const term = q.trim()
  const list = useQuery({
    queryKey: ['members', 'pick', term],
    queryFn: () => get<MemberRow[]>(`/members?q=${encodeURIComponent(term)}`),
    enabled: term.length > 0,
    placeholderData: (p) => p,
  })
  const rows = term ? (list.data ?? []).slice(0, 8) : []

  return (
    <DialogContent className="sm:max-w-md">
      <DialogHeader>
        <DialogTitle>{title}</DialogTitle>
        {description && <DialogDescription>{description}</DialogDescription>}
      </DialogHeader>
      <div className="relative">
        <SearchIcon className="text-muted-foreground absolute inset-y-0 start-3 my-auto size-4" />
        <Input autoFocus className="ps-9" placeholder="نام یا موبایل" value={q} onChange={(e) => setQ(e.target.value)} />
      </div>
      <ul className="-mx-2 grid max-h-80 gap-0.5 overflow-y-auto">
        {rows.map((m) => (
          <li key={m.id}>
            <button
              type="button"
              disabled={busy}
              onClick={() => onPick(m)}
              className="hover:bg-accent focus-visible:bg-accent flex w-full items-center gap-3 rounded-lg px-3 py-2.5 text-start outline-none disabled:opacity-50"
            >
              <span className="min-w-0 flex-1">
                <span className="block truncate font-medium">{m.full_name}</span>
                <span className="text-muted-foreground text-xs">
                  {GENDER_LABEL[m.gender]} · <span dir="ltr">{faDigits(m.phone)}</span>
                </span>
              </span>
              <StatusBadge status={m.status} />
            </button>
          </li>
        ))}
        {term && list.data && rows.length === 0 && <li className="text-muted-foreground px-3 py-6 text-center text-sm">عضوی پیدا نشد.</li>}
        {!term && <li className="text-muted-foreground px-3 py-6 text-center text-sm">برای جستجو تایپ کنید.</li>}
      </ul>
    </DialogContent>
  )
}
