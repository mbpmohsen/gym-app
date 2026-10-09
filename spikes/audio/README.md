# audio spike (milestone 0, throwaway)

Can a browser tab keep playing sounds pushed over SSE for hours: minimized, screen locked, after sleep?

```sh
cd spikes/audio
cargo run --release -- 60      # one sound every 60 s
```
Open http://127.0.0.1:7470 , click «شروع», then leave it. Results: `audio-spike.log`
(one line per sound: `PLAYED`, `FAILED <reason>`, `MISSING`, plus `tab hidden/visible`).

Finding so far: plain `<audio>` elements failed randomly with `NotAllowedError` even on a
visible tab; one Web Audio `AudioContext` unlocked by the click works.
