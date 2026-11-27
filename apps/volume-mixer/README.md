# Volume Mixer (`slopos-volume-mixer`)

The standalone Volume Mixer. It shows one fader and a Mute box for the default
output, the default input, and each application that is playing sound. The top
bar's sound item opens the same mixer as a popover.

![Volume Mixer with an application playing](screenshots/volume-mixer-app.png)

*Captured 2026-11-06 with `scripts/qa/volume-mixer.sh`, using the
`scripts/qa/fake-pactl` fixture as the sound server (no real sound device).*

![Volume Mixer with the output muted](screenshots/volume-mixer-muted.png)

*Captured 2026-11-27 with the same QA script. The output is muted: the fader
keeps its level, the label reads "Muted", and the focused check box has its
ring.*

## Features

- A fader for the output, the input and each playing application, each with a
  percentage and a Mute box.
- Dragging a fader or clicking Mute sends the change to the sound server.
- **Without a sound server** the window says "Sound unavailable" and draws no
  levels. It does not show fake values.
- **Retry:** press R to ask the sound server again. A mixer opened before the
  server started can recover that way.
- **Top bar popover:** a left-click on the sound item opens the mixer under the
  item. Click the item again, press Escape, or press outside to close it.

## How it works

- `crates/slopos-mixer` holds the model. It turns an audio snapshot into
  channels and status, and provides the commands and the view (`MixerView`).
  The window and the top bar both use it.
- The sound data and commands come from the audio provider in
  `slopos-services`, which runs `pactl`, the PulseAudio-compatible client that
  PipeWire and PulseAudio both serve. With no server it reports "unavailable".
- `src/main.rs` is the window and the key handling.

## Running it

```sh
cargo run --locked -p slopos-volume-mixer
slopos-volume-mixer                              # after install.sh
```

## QA

```sh
bash scripts/qa/volume-mixer.sh     # needs Xvfb, openbox, xdotool and ImageMagick
bash scripts/qa/topbar-mixer.sh     # the top bar popover
```

`scripts/qa/fake-pactl` is a fixture that stands in for the sound server, so
these checks run without a sound card.

## Tests

```sh
cargo test -p slopos-mixer -p slopos-volume-mixer
```

## Status and limits

- Only the default output and input are shown. To pick another device, use
  Control Panels > Sound.
- The sound checks above use the fixture. They have not been run against a
  real PipeWire or PulseAudio server on hardware.
