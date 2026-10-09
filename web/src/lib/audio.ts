// Sound playback through ONE Web Audio context.
//
// Why not <audio>: in the milestone-0 spike, <audio>.play() randomly failed with
// NotAllowedError even on a visible tab. A single AudioContext, resumed during a
// user gesture (the first click anywhere), keeps playing afterwards, also while
// the tab is hidden. See spikes/audio.

export type Sound = 'welcome' | 'goodbye' | 'end-of-tuition' | 'wrong-shift'
export type Voice = 'male' | 'female'

const SOUNDS: Sound[] = ['welcome', 'goodbye', 'end-of-tuition', 'wrong-shift']

let ctx: AudioContext | null = null
let voice: Voice | null = null
let buffers: Partial<Record<Sound, AudioBuffer>> = {}
const listeners = new Set<() => void>()

function context(): AudioContext {
  if (!ctx) {
    ctx = new AudioContext()
    ctx.onstatechange = () => listeners.forEach((l) => l())
  }
  return ctx
}

export function audioReady(): boolean {
  return ctx?.state === 'running'
}

export function onAudioChange(fn: () => void): () => void {
  listeners.add(fn)
  return () => listeners.delete(fn)
}

/** Must be called from a user gesture (click / keydown) the first time. */
export async function unlockAudio(): Promise<void> {
  const c = context()
  if (c.state !== 'running') await c.resume()
  listeners.forEach((l) => l())
}

/** Loads the given voice's files (cached until the voice changes). */
export async function loadVoice(v: Voice): Promise<void> {
  if (voice === v && Object.keys(buffers).length === SOUNDS.length) return
  const c = context()
  const loaded: Partial<Record<Sound, AudioBuffer>> = {}
  await Promise.all(
    SOUNDS.map(async (s) => {
      const bytes = await (await fetch(`${import.meta.env.BASE_URL}voices/${v}/${s}.mp3`)).arrayBuffer()
      loaded[s] = await c.decodeAudioData(bytes)
    }),
  )
  buffers = loaded
  voice = v
}

export async function play(sound: Sound): Promise<void> {
  const c = context()
  // after sleep/lock the browser may suspend the context; resuming needs no new gesture
  if (c.state !== 'running') await c.resume()
  const buffer = buffers[sound]
  if (!buffer || c.state !== 'running') throw new Error(`cannot play ${sound} (audio ${c.state})`)
  const src = c.createBufferSource()
  src.buffer = buffer
  src.connect(c.destination)
  await new Promise<void>((resolve) => {
    src.onended = () => resolve()
    src.start()
  })
}
