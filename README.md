# SLOPOS-I

SLOPOS-I is a sovereign Linux desktop environment written in Rust and built
around its own Wayland compositor, shell, toolkit, SDK, applications and local
services.

Its target is explicit:

> **Become a 100/100 production desktop environment that genuinely competes
> with KDE Plasma and GNOME as a dependable daily driver.**

SLOPOS-I is not a theme running on another desktop and has no production
fallback to another compositor. It combines the compactness and directness of
classic Macintosh/System 7 interaction with the architecture, compatibility,
accessibility, security and reliability expected from a modern Linux desktop.

## Current status

SLOPOS-I is **not yet production-ready**. The latest evidence-backed audit rates
the current product at **63/100: a strong custom desktop alpha**.

The repository already contains substantial real implementation:

- a SLOPOS-owned Wayland compositor and private session socket;
- DRM/KMS, nested and headless backend work;
- XDG toplevel, popup and presentation-state handling;
- a session supervisor;
- a custom desktop shell, global menu and Dock;
- SLOPOS Spaces foundations;
- a native widget toolkit and application SDK;
- file manager, Settings, TextEdit, Terminal, Preview and software-management
  applications;
- local OCR and subject-segmentation through SLOPOS Vision;
- Linux CI covering workspace build, tests, Clippy, release build, formatting,
  lockfile consistency and a headless compositor protocol gate.

The major remaining blockers are production rendering, complete compositor
hardware and application compatibility, SLOPOS Spaces product integration,
authoritative Settings services, accessibility, application depth, security,
packaging, upgrades, recovery and long-running reliability.

See [`TRUTH.md`](TRUTH.md) for the current audit and scores. See
[`AGENTS.md`](AGENTS.md) for the complete 100/100 production programme.

## Product principles

- **Sovereign:** the SLOPOS compositor owns the desktop session and window policy.
- **Classic but modern:** compact and direct rather than nostalgic at the cost
  of usability.
- **User-controlled:** window zoom behaviour, fonts, Spaces and desktop policies
  belong in the user’s hands.
- **Local-first:** sensitive services such as SLOPOS Vision run locally by
  default.
- **Evidence-driven:** source scaffolding and passing unit tests are not treated
  as proof of production readiness.
- **Accessible:** keyboard-only and assistive-technology operation are release
  requirements.
- **Compatible:** normal Wayland and XWayland applications must work without a
  second compositor.
- **Recoverable:** installation, upgrade, rollback, diagnostics and recovery are
  part of the product.

## Architecture

```text
Display manager / TTY
└── slopos-session
    ├── slopos-compositor
    │   ├── DRM/KMS output and presentation
    │   ├── input and cursor management
    │   ├── window management and SLOPOS Spaces
    │   ├── layer-shell and work areas
    │   ├── XWayland integration
    │   └── private Wayland socket
    ├── slopos-shell
    ├── first-party SLOPOS applications
    ├── third-party Wayland/XWayland applications
    └── session services such as slopos-visiond
```

In nested development, the host compositor sees one outer SLOPOS output window.
All SLOPOS shell and application surfaces connect to the private compositor
socket inside it.

## What 100/100 means

The final production claim requires more than feature labels. Every major
subsystem must have implementation, automated tests, runtime evidence,
applicable physical-hardware evidence, failure-path coverage, performance
budgets and exact-commit reproducibility.

The programme covers:

1. Linux compositor completion;
2. retained GPU rendering, text, images and fonts;
3. shell, Dock, notifications and full SLOPOS Spaces;
4. authoritative system services and Settings;
5. production first-party applications;
6. broad Wayland, XWayland and portal compatibility;
7. accessibility and localisation;
8. security, application trust and permissions;
9. performance, soak testing and recovery;
10. packaging, upgrades and release engineering;
11. a POSIX-portable boundary and eventual FreeBSD support without forking the
    desktop.

SLOPOS-I may call itself production-ready only when `TRUTH.md` supports that
claim with current evidence and no release-blocking contradiction.

## Workspace

Core crates include:

- `slopos-session`, `slopos-compositor`, `slopos-shell`;
- `slopos-render`, `slopos-kit`, `slopos-sdk`, `slopos-bus`;
- `slopos-fonts`;
- `slopos-vision`, `slopos-vision-protocol`, `slopos-vision-client`,
  `slopos-visiond`.

First-party applications live under `apps/`.

## Build

A stable Rust toolchain and the Linux development dependencies listed under
`packaging/deps/` are required.

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --all-features --locked
cargo build --release --workspace --locked
```

Passing these commands proves build and test health, not full desktop runtime or
hardware compatibility. Exact verified results belong in `TRUTH.md`.

## Run a nested development session

```bash
cargo build --release -p slopos-session -p slopos-compositor -p slopos-shell
SLOPOS_BACKEND=nested ./scripts/start-slopos-i
```

On software-rendered virtual machines, environment overrides may be necessary:

```bash
LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe \
  SLOPOS_BACKEND=nested ./scripts/start-slopos-i
```

Check the current compositor help and launch script before relying on backend
arguments:

```bash
./target/release/slopos-compositor --help
```

## Documentation

The repository deliberately has only three Markdown files:

- [`README.md`](README.md) — public introduction and quick start;
- [`AGENTS.md`](AGENTS.md) — complete architecture, requirements, implementation
  order and acceptance criteria;
- [`TRUTH.md`](TRUTH.md) — current audit, evidence, maturity and defect ledger.

Do not add parallel roadmap or audit Markdown files. Raw QA artefacts belong
under `artifacts/qa/`.

## Naming

The native nearby-transfer feature is **SLOPOS Share**. Do not use Apple’s
AirDrop name in SLOPOS product copy. Other inherited application names must also
receive a deliberate public naming and trademark review before release.

## SLOPOS Vision

SLOPOS Vision provides local OCR and subject segmentation through a reusable
engine, typed local protocol, client and session daemon. Models are not silently
uploaded or downloaded at runtime. Distribution, measured accuracy,
acceleration and complete file-manager/Preview workflows remain active work and
are tracked in `TRUTH.md`.

## Licensing

First-party source and original assets are MIT-licensed:

```text
Copyright (c) 2026 Palaash Atri
```

Third-party crates, system components, fonts, codecs and model weights retain
their own licences. See `LICENSE`, `COPYRIGHT`, `THIRD_PARTY_LICENSES.txt`,
`deny.toml` and `models/vision/manifest.toml`.
