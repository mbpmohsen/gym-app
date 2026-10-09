// Image URLs that come from face-service through gym-server. The demo build has
// no camera: it draws abstract placeholders instead (never a real face).

export function snapshotSrc(file: string): string {
  return import.meta.env.VITE_DEMO ? demoAvatar(file) : `/api/snapshots/${file}`
}

export function previewSrc(key?: number): string {
  return import.meta.env.VITE_DEMO ? demoPreview() : `/api/face/preview${key === undefined ? '' : `?k=${key}`}`
}

const HUES = [210, 340, 160, 30, 270, 190, 10, 120]
const svg = (s: string) => `data:image/svg+xml;utf8,${encodeURIComponent(s)}`

function demoAvatar(key: string): string {
  const n = [...key].reduce((a, c) => (a * 31 + c.charCodeAt(0)) >>> 0, 7)
  const h = HUES[n % HUES.length]
  const unknown = key.startsWith('u')
  return svg(
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 80 80"><rect width="80" height="80" fill="hsl(${h} 40% ${unknown ? 32 : 46}%)"/>` +
      `<circle cx="40" cy="32" r="15" fill="hsl(${h} 55% 86%)"/><path d="M14 80c2-18 13-27 26-27s24 9 26 27z" fill="hsl(${h} 55% 86%)"/>` +
      (unknown ? '<text x="40" y="38" font-size="17" text-anchor="middle" fill="#334155" font-family="sans-serif">?</text>' : '') +
      '</svg>',
  )
}

function demoPreview(): string {
  return svg(
    '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 480"><rect width="640" height="480" fill="#111827"/>' +
      '<circle cx="320" cy="200" r="78" fill="#374151"/><path d="M180 480c10-110 70-165 140-165s130 55 140 165z" fill="#374151"/>' +
      '<rect x="228" y="100" width="184" height="210" fill="none" stroke="#22c55e" stroke-width="4" rx="6"/>' +
      '<text x="320" y="455" font-size="20" text-anchor="middle" fill="#9ca3af" font-family="sans-serif">demo camera</text></svg>',
  )
}
