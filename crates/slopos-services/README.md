# slopos-services

The system-service adapters. Each adapter connects to one system service
(NetworkManager, the sound server, BlueZ and so on) and gives the SLOPOS
programs a typed snapshot of its state, plus commands to change it.

Every adapter reports **unavailable** with a reason when its service is not
running. It never invents a value (AGENTS.md rule 1). A program shows the reason
and disables the matching controls.

## Adapters

| Module | Service | What it gives |
|---|---|---|
| `network.rs` | NetworkManager | Connection state, Wi-Fi networks, join, disconnect, forget |
| `audio.rs` | PipeWire or PulseAudio (through `pactl`) | Output and input devices, per-application streams, volume and mute |
| `bluetooth.rs` | BlueZ | Adapter power, discovery, device list, pair, connect, forget |
| `brightness.rs` | `/sys/class/backlight`, changes through logind | Backlight level and bounds |
| `display.rs` | XRandR | Outputs, modes, rotation, placement, and applying a layout plan |
| `input.rs` | XKB and XInput 2 | Keyboard layout, repeat, pointer speed, natural scrolling |
| `media.rs` | MPRIS | Players and their state |
| `power.rs` | UPower | Battery level and state |
| `storage.rs` | UDisks | Removable volumes |
| `timedate.rs` | systemd-timedated | Time zone and automatic time |

## Using it

Programs use these adapters through the same pattern: start the client, read
its snapshot stream, and send commands. The Control Panels app and the top
bar's status items are examples.

```toml
[dependencies]
slopos-services = { path = "../../crates/slopos-services" }
```

## Tests

```sh
cargo test -p slopos-services
```

Some tests start a private `dbus-daemon`, so it must be on the path. Seven tests
are marked `#[ignore]` because they need a real system service. They are not run
by default, and their results are not counted as passing.
