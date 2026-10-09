# web

UI of gym-app: React + TypeScript + Vite + Tailwind v4 + **shadcn/ui** (new-york, Radix), RTL Persian,
Jalali dates via `Intl`. Built into `dist/` and embedded in `../server` (see `../server/README.md`).

- **shadcn:** components live in `src/components/ui/`. `components.json` is set up, so on a machine
  with internet `npx shadcn@latest add <component>` works as usual.
- **Themes:** mode (light / dark / system) × accent (6 colors), in `src/index.css` (`.dark`,
  `[data-accent=...]`) and `src/lib/theme.ts`. Saved per computer in localStorage; `index.html`
  applies it before first paint.
- **Status colors** are fixed in every theme and carry meaning: `success` = ok,
  `warning` = warning, `destructive` = end of tuition / wrong shift / error.
- **Sound** plays through one Web Audio context (`src/lib/audio.ts`), not `<audio>` (see `../spikes/audio`).
