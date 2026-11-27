# slopos-mixer

The model behind the Volume Mixer. It is a library, used by both the standalone
app ([`apps/volume-mixer`](../../apps/volume-mixer/README.md)) and the top bar's
popover ([`crates/slopos-topbar`](../slopos-topbar/README.md)).

## What it does

- **Model** (`src/model.rs`): turns an audio snapshot from `slopos-services` into
  channels: the default output, the default input, and each application that is
  playing sound. Each channel has a level, a mute state and a status.
- **Provider** (`src/provider.rs`): holds the connection to the sound server. It
  reconnects on request, so a server that starts later is picked up. When no
  server is available it reports that instead of showing levels.
- **View** (`src/view.rs`): `MixerView`, the toolkit control that draws the
  faders and Mute boxes, handles keyboard and pointer input, and provides the
  accessibility nodes.

## Using it

```toml
[dependencies]
slopos-mixer = { path = "../../crates/slopos-mixer" }
```

## Tests

```sh
cargo test -p slopos-mixer
```

The tests cover the channel model, the status text, the fader and Mute
behaviour, focus, and the accessibility nodes.
