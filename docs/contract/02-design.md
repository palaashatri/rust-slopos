<!-- Part of the SLOPOS-I engineering contract; index: ../CONTRACT.md -->

# 5. Canonical visual reference

The canonical visual/component reference is:

- **Classic Macintosh UI Kit (Community)**
- Figma file key: `LGMlwNCoVdakZxDBvPKg1W`
- root node: `0:1`

Treat the Figma file as a component specification, not vague inspiration.

Observed examples from the 2026-09-24 audit include:

- menu bar example: 19 px high;
- menu row example: 16 px high;
- large Finder item example: 71×44 px;
- large Finder icon example: 32×32 px;
- large Finder label region example: 71×12 px;
- regular button example: 80×20 px;
- default button example: 88×28 px;
- secondary button example: 80×16 px;
- explicit Rest / Pressed / Disabled button states;
- Action / Hierarchical menu row variants;
- Active / Hover / Disabled menu states;
- Active / Inactive title-bar variants;
- 1-bit and 8-bit reference variants.

These measurements are examples only. The implementation must extract all relevant component data into the machine-readable specification before claiming exact conformance.

## 5.1 Clean-room rule

SLOPOS may reproduce:

- geometry;
- hierarchy;
- spacing;
- density;
- interaction affordances;
- state treatment;
- visual grammar.

Do not ship:

- Apple logos;
- proprietary Macintosh artwork;
- copied proprietary icons;
- proprietary Apple fonts;
- proprietary sounds;
- copied wallpapers;
- copied documentation text.

Third-party assets or fonts require verified redistribution licenses.

If a reference font cannot legally be redistributed, choose or create a redistributable metrically appropriate replacement and document the decision.

## 5.2 SLOPOS Design Bible — design language, principles, tokens and review

**Status:** the rules marked **[SLOPOS-I]** bind current Platinum Classic work. The rules marked **[FUT-UI]** describe the future themeable toolkit (§21.7.1). They are not current release gates and no implementation exists for them yet. This section is the single home of the SLOPOS Design Bible. Under §34 it lives here, not in a separate `DESIGN.md` or a design-bible directory.

**Provenance.** The structure is ported from the [Loom Design Bible](https://github.com/palaashatri/rust-loom/tree/main/loom-design-bible): principles with anti-examples, one canonical token source, theme matrix, anti-pattern catalogue, design-review checklist and governance. SLOPOS adapts that method. It does **not** import Loom's palette, type scale, spacing, motion values or Slint components. SLOPOS values come from its own clean-room Classic dataset (§10) and, later, a separately reviewed Modern dataset. Loom keeps its own Design Bible as the authority for Loom applications on every platform (§21.7.2).

### 5.2.1 Design inspirations and how each one is used

SLOPOS learns from several references. Each reference has one bounded role. No reference is an asset source, with one explicit exception: the pinned Lucide symbolic SVGs under `assets/icons/symbolic/lucide/` are licensed third-party reference assets that ship only with their upstream `ISC`/`MIT` licence and `NOTICE.txt`, and they are not wired into the running desktop. Only §5's Figma kit is a measured acceptance authority for SLOPOS-I.

| Reference | What SLOPOS takes | What SLOPOS never takes | Authority |
|---|---|---|---|
| **Classic Macintosh UI Kit** (Figma `LGMlwNCoVdakZxDBvPKg1W`), System 7 / Mac OS 8 **Platinum** | Geometry, density, hierarchy, state treatment, bevels and patterns, menu and title-bar grammar | Apple logos, artwork, icons, fonts, sounds, wallpapers, documentation text (§5.1) | **[SLOPOS-I] binding** through `qa/spec/classic/` |
| **[Sinty / Singularity Desktop](https://sinty.dev/)** | Modern direction: calm dark workspace, compact unified top bar, floating Dock, restrained controls, colorful but original app identity | Artwork, stylesheets, icons, branding | **[FUT-UI]** inspiration for SLOPOS Modern (§21.7.1) |
| **Windows XP-class personalization** | A north star: whole visual styles, layout, icon packs, cursors, sounds and accents can each change without touching app code | Visual style, assets or names | **[FUT-UI]** customization scope (§21.7.1) |
| **Loom Design Bible** | Bible structure, review discipline, token architecture, creative-suite semantic roles for the Loom ↔ SLOPOS adapter | Loom's numeric tokens as SLOPOS values; forcing Loom onto SLOPOS widgets | Method only; Loom stays independent (§21.7.2) |
| **macOS-like usability traits** (global app menu, Dock, Finder/Explorer-like navigation) | Usability conventions, built as original code | Proprietary visual assets or copied behavior text | §14, §21.7.4 |
| **Apple Vision / Live Text / Visual Look Up-style workflows** | Interaction ideas for local, opt-in text and image features | Claims of feature or model parity | **[FUT-VISION]** §21.7.3 |
| **Lucide symbolic glyphs** (`assets/icons/symbolic/`) | Licensed symbolic prototypes for the 12 app-list roles | Use as colorful brand icons; dropping the license notices | Reference set; not wired into the running desktop |
| **WCAG 2.2 AA / EN 301 549 V4.1.1** | The accessibility benchmark for the shell and first-party apps (§5.2.10, §21.10.4) | A legal conformance claim without an audit | Design and test target |
| **Vinix / VitruvianOS** | System lessons that affect experience: supervised sessions, coherent services, whole-desktop memory discipline | Code, or treating their benchmark claims as verified | §21.8.1 |

A new inspiration is added to this table with the same columns before it influences any requirement. Removing one requires an owner decision. Refining its wording or scope does not.

### 5.2.2 Design language

SLOPOS is **compact, precise, truthful, calm, coherent, personal and fast.**

- **Compact:** Classic density. Small, exact controls; no inflated padding, no card grids.
- **Precise:** pixel-exact 1× geometry with deterministic integer HiDPI scaling (§9.2, §11).
- **Truthful:** the desktop never shows state that a provider did not report (AGENTS.md rule 1, §22).
- **Calm:** chrome supports the work. No decorative motion, badges or noise.
- **Coherent:** one toolkit, one action model and one theme pipeline for the shell and every first-party app (§2, §12).
- **Personal [FUT-UI]:** look, layout, icons and preferences change independently; behavior stays the same.
- **Fast:** input feedback within one frame; no UI-thread waits on D-Bus, disk, network or decoding (§21.9).

### 5.2.3 Principles

When principles conflict, the lower number wins, with two exceptions that always win: **accessibility (principle 8)** and **truthful state (principle 6)**.

1. **Content and task first.** Windows, documents and the user's files dominate; the menu bar, Dock and panels support them. *Anti-examples:* a permanently expanded control strip; decorative headers; a shell surface that cannot be hidden or collapsed where the layout profile allows it.
2. **Calm over busy.** Reduce until further reduction would remove capability, then disclose the rest progressively. *Anti-examples:* every applet always visible; pulsing "new" dots; borders around every control; gradients and shadows as decoration in Classic.
3. **Direct manipulation first.** Drag icons, resize windows, reorder Dock items and drag files between windows. *Anti-examples:* reordering only through up/down buttons; resize only through numeric fields; no drag feedback.
4. **Progressive disclosure.** Use this order: (1) the object itself, (2) the window's own controls, (3) the global menu and context menus, (4) Control Panels, (5) advanced panels, (6) automation through typed actions (§21.6). *Anti-examples:* a common command buried in layer 5; one feature exposed in three layers at once; modal dialogs used as a disclosure mechanism.
5. **Predictability across the desktop.** The same shortcut, component and gesture behave the same in the shell and in every first-party app, in every theme and layout profile. *Anti-examples:* two checkbox styles; per-app undo models; a theme that changes what a command does.
6. **Truthful feedback and state (always wins).** Every action gives observable, accurate, timely feedback. Missing providers show "unavailable". *Anti-examples:* fake progress; sample SSIDs or IP addresses; "saved" before the write is flushed; a disabled control that gives no reason; an About dialog that overstates readiness.
7. **Motion with meaning.** Every animation answers a usability question: where did this come from, where is it going, what changed. Platinum Classic is essentially motionless (instant state changes and a brief menu-choice confirmation). Modern motion values come only from its theme dataset. *Anti-examples:* bouncing Dock icons by default; windows that fly in; animation that cannot be reduced.
8. **Accessibility is release-blocking (always wins).** Full keyboard navigation, visible focus, AT-SPI names, roles and states, logical focus order, scalable text, reduced motion and non-color status are requirements (§9.14, §25). A theme or layout may never disable them.
9. **Depth, never featurelessness.** Minimal does not mean missing: power-user settings exist and are reachable through disclosure. *Anti-examples:* an "easy" mode that cannot do the job; an empty "Advanced" pane.
10. **Performance is a feature.** Damage-driven painting, bounded caches, lazy providers, no idle polling. Edge budgets are in §21.9. *Anti-examples:* the shell freezing while NetworkManager answers; thumbnails decoded on the UI thread.
11. **Character without imitation.** Classic carries Platinum's grammar and Modern carries Sinty-inspired restraint, both through original assets. *Anti-examples:* copied Apple or Sinty artwork; skeuomorphic textures; emoji as UI.
12. **Every default is deliberate.** Default theme, layout profile, Dock contents, font scale, window placement and shortcuts are designed and documented. *Anti-examples:* defaults chosen because a placeholder was needed; a Dock pinned with apps that are not installed.

### 5.2.4 Area map — where each design concern is owned

Loom splits its bible into area documents. SLOPOS keeps one owner per concern inside this contract and its machine-readable datasets:

| Concern (Loom bible document) | SLOPOS owner |
|---|---|
| Tokens, color, typography, metrics, patterns (`DESIGN_TOKENS`, `COLOR`, `TYPOGRAPHY`, `SPACING`) | §9.2, §9.13 and `qa/spec/classic/*.json` (§10). Prose never repeats numeric values. |
| Iconography (`ICONOGRAPHY`) | §9.12; icon roles and packs in §21.7.1 layer 3 |
| Components and state matrices (`COMPONENTS`) | §9.4–9.11, completion in §35 |
| Windows, layout, sidebars, inspectors (`WINDOWS`, `LAYOUT`, `SIDEBARS`, `INSPECTORS`) | §19 native application anatomy, §23 window management |
| Menus and command palette (`MENUS`, `COMMAND_PALETTE`) | §9.9, §12.2–12.3, §14.1, §14.6 |
| Dialogs and notifications (`DIALOGS`, `NOTIFICATIONS`) | §9.10, §14.4, §14.7, §18.15–18.16 |
| Selection, drag and drop, keyboard, pointer (`SELECTION`, `DRAG_AND_DROP`, `KEYBOARD`, `POINTER_AND_PEN`) | §9.2 `focus`/`selection`, §14.8, §26 |
| Accessibility (`ACCESSIBILITY`) | §9.14, §25 |
| Theming (`THEMING`) | §5.2.6 below and §21.7.1 |
| Visual QA (`VISUAL_QA`) | §11, §27 |
| Performance (`PERFORMANCE`) | §21.9 |
| Anti-patterns, design review, acceptance (`ANTI_PATTERNS`, `DESIGN_REVIEW`, `UX_ACCEPTANCE_CHECKLIST`) | §5.2.7–5.2.8 below, §35–36 |
| Canvas, timeline, spreadsheet, document editor (`CANVAS`, `TIMELINE`, `SPREADSHEET`, `DOCUMENT_EDITOR`) | `NOT_USED` in SLOPOS: these surfaces belong to Loom's own bible. SLOPOS owns only the window chrome and services around them. |

### 5.2.5 Token architecture

- **Three layers:** *primitive* values (exact colors, sizes, patterns), *semantic* roles (for example `surface`, `ink`, `ink-disabled`, `accent`, `focus`, `selection`, `border`, `danger`, `menu-bar`, `title-bar-active`), and *component* recipes (button plate, menu row, title bar, scrollbar). Components consume semantic roles and recipes. They never read primitives directly.
- **One source per theme:** **[SLOPOS-I]** Platinum Classic values come only from `qa/spec/classic/`. Many of its entries are still `SOURCE_PENDING`/`UNKNOWN`. Until extraction completes, implementations must not invent replacement values or claim exact conformance (§10). **[FUT-UI]** SLOPOS Modern gets its own dataset with the same schema and semantic role names. Third-party themes supply the same roles in a validated declarative package.
- **Rules:**
  1. First-party UI uses `slopos-ui` components and semantic roles, never local literals. A literal in a component is a defect.
  2. Theme variants change values, never role names.
  3. A new role is added only when no existing role can express the requirement, and in the same change as the dataset, schema and conformance fixture.
  4. Interactive state changes never alter layout geometry.
  5. The accent is reserved for focus, selection, default and primary actions and meaningful emphasis. It is not background decoration.
  6. Classic uses Platinum bevels, patterns and borders for depth. Modern may define elevation only through its dataset.
  7. Content-specific roles (wallpaper, document, media) never decorate chrome.

### 5.2.6 Theme matrix and orthogonal modes

| Configuration | Status | Notes |
|---|---|---|
| **Platinum Classic** | **[SLOPOS-I]** default and only shipping style | §5 is the authority; 1-bit/8-bit reference variants are `OPTIONAL_VARIANT` in the dataset. |
| Classic high contrast | **[SLOPOS-I] only if fully specified** (§25) | Must be specified in the dataset with contrast evidence before it is offered. |
| **SLOPOS Modern** (dark default, optional light) | **[FUT-UI]** | Sinty-inspired, original assets, same semantic roles. |
| Third-party / community styles (an optional Loom-inspired one included) | **[FUT-UI]** | Signed or user-trusted data packages. No scripts. Live preview, rollback and restore defaults. |
| Reduced motion, text scale, reduced transparency | Orthogonal modes | Apply over any style. They are not themes. |
| Desktop layout profile (global menu vs. panel, Dock vs. taskbar) | **[FUT-UI]** orthogonal | Chosen independently of the visual style (§21.7.1 layer 2). |
| Icon pack (symbolic and brand roles) | **[FUT-UI]** orthogonal | Stable app IDs and icon roles, with semantic fallback (§21.7.1 layer 3). |

Rules:

- A style change applies without restart or data loss. Document content and wallpapers are not re-themed.
- No style may remove focus indicators, contrast floors, security or permission indicators, or keyboard paths.
- Every style that ships must pass the same gallery, geometry, accessibility and scale gates (§11, §25, §27). Adding a style means adding a dataset and its QA, never new components or per-app switches.
- Loom and other Slint, GTK or Qt apps receive styles only through explicit adapters (§21.7.1 toolkit contract). They never get a claim of exact cross-toolkit parity.

### 5.2.7 Anti-patterns

Each of these is a defect no matter how good it looks in isolation. Reviewers reject it without argument. A needed exception is a reviewed contract change, never a quiet workaround in a component.

1. **Invented system state:** sample devices, SSIDs, levels or readiness text shown as real.
2. **Raw-GTK first-party UI:** new shell or app UI that bypasses `slopos-ui` and appkit (§2.2).
3. **Per-app drift:** the same control re-implemented or restyled per app, including per-app theme switches.
4. **Modern chrome in Classic:** rounded cards, soft shadows, pills or gradients in Platinum Classic surfaces.
5. **Copied proprietary assets:** Apple, Sinty or other third-party artwork, icons, fonts or sounds without verified rights.
6. **Invisible focus:** focus rings removed "for polish", in any theme.
7. **Color-only status:** state conveyed by color alone, without text, an icon, shape or position.
8. **Hover-only functionality:** actions that exist only under the pointer.
9. **Tiny unlabeled icons:** icon-only controls without an accessible name, tooltip and menu equivalent.
10. **Decorative animation:** motion that answers no question, or that cannot be reduced.
11. **UI-thread blocking:** synchronous D-Bus, disk, network, thumbnail or decode work in the shell or an app.
12. **Fake progress, uncancellable work:** progress that does not reflect real work; long jobs without cancel.
13. **Stealing focus:** background completion that raises windows or moves the caret.
14. **Empty dead end:** an empty state with no action path.
15. **Silent failure:** an action that fails without visible, accessible feedback.
16. **Nested scroll traps:** scroll regions inside scroll regions.
17. **Destructive default:** Return activating the destructive choice in an alert.
18. **Executable themes:** theme or icon packages that run code, change permissions or disable required indicators.
19. **Screenshot as proof:** a capture or mockup offered as conformance, accessibility or compatibility evidence (§33).
20. **Obscured focus:** the Dock, menu bar, an OSD, a notification or an always-on-top panel fully hides the focused control.
21. **Drag-only operation:** an action (Dock reorder, window move/resize, file move) that has no keyboard or single-click alternative.
22. **Hidden privacy state:** a theme, layout or app that can hide the camera, microphone, screen-capture or automation-in-use indicator.
23. **Dark patterns:** pre-checked consent, confirm-shaming, disguised ads, settings that silently reset, or a "close" that does something else.
24. **Unlabelled AI output:** generated text, OCR or suggestions applied or shown without saying they are machine-generated, or applied without review.

### 5.2.8 Design review checklist

Every user-visible change, and every change to this section or a spec dataset, is checked before merge. Each item needs evidence: a test, a fixture, a dataset diff or a labelled capture.

- **Contract integrity:** values come from the theme dataset; no new literals; role names unchanged or added through §5.2.5 rule 3; no required `UNKNOWN` entry claimed as conformant.
- **Visual and layout:** Classic geometry is exact at 1× and integer HiDPI (§11); no anti-pattern from §5.2.7; icons follow §9.12 roles and sizes; typography uses §9.13 roles.
- **Interaction and motion:** direct manipulation and the disclosure order (§5.2.3) are respected; motion comes from tokens and is reduced-motion safe; selection and focus are visible.
- **Accessibility:** complete keyboard path; AT-SPI role, name and state; focus order and focus return; text-scale check; non-color status (§9.14, §25).
- **Truthfulness and performance:** provider-backed state only; determinate progress where possible; a cancel path exists; no UI-thread blocking; empty and error states are specified.
- **2027 baseline:** the change meets §5.2.10. Each new WCAG 2.2 criterion it touches is listed with its test or recorded as `NOT_RUN`.
- **Cross-project impact:** a change to semantic roles or icon roles lists its effect on the Loom adapter (§21.7.2) and on the PARAMA backend (§21.7.5) as `NONE`, `ADAPTER_UPDATE_NEEDED` or `NOT_RUN`.
- **Verification:** named tests and gallery fixtures; captures for default, pressed, disabled, focused and error states; unrun gates recorded as `NOT_RUN` in Part II.

### 5.2.9 Shared semantics across SLOPOS, Loom and PARAMA

- **SLOPOS ↔ Loom [FUT-LOOM]:** the theme-token and chrome adapter maps SLOPOS semantic roles (surface, ink, accent, focus, selection, border, status, menu and title-bar roles, text scale, reduced motion, high contrast) onto Loom's semantic roles in `loom-design-bible/tokens/loom.toml`. The mapping lives in a versioned adapter, never in either project's core tokens. Where no role matches, Loom keeps its own default. Document content is never re-themed.
- **SLOPOS ↔ PARAMA [FUT-PARAMA]:** PARAMA consumes the same theme datasets, layout profiles, icon manifests and app IDs through its `PlatformBackend`. It has no second theme engine (§21.7.5; PARAMA `AGENTS.md` §5.1).
- **Independence:** each repository keeps its own release gates and evidence ledger. A change in one project's tokens never silently changes another's. The adapter version records what was mapped and tested.

### 5.2.10 2027 UX baseline

**Accessibility reference.** WCAG 2.2 AA, as EN 301 549 V4.1.1 clause 11 applies it to non-web software, is the design benchmark (§21.10.4). It adds to §9.14 and §25 and replaces nothing there. The WCAG 2.2 criteria that matter most for a desktop are:

| WCAG 2.2 criterion | SLOPOS rule |
|---|---|
| 2.4.11 Focus Not Obscured (AA) | Dock, menu bar, OSDs, notifications and floating panels never fully cover the focused control. Windows are placed and scrolled so focus stays visible; the OSD layer yields to focus. |
| 2.4.13 Focus Appearance (AAA, adopted as a goal) | Focus rings are drawn from the theme's `focus` role with enough area and contrast in every style. No style may make them thinner than its dataset's minimum. |
| 2.5.7 Dragging Movements (AA) | Every drag has an alternative: Dock reordering through the context menu or keyboard; window move and resize through the window menu and keyboard (Openbox supports these); file moves through Cut/Paste or Move To. |
| 2.5.8 Target Size (Minimum) (AA) | See the Classic density rule below. |
| 3.2.6 Consistent Help (A) | Help stays in the same place: the last menu of the global menu bar, plus SLOPOS Help (§17.5). |
| 3.3.7 Redundant Entry (A) | Dialogs and Control Panels do not ask again for information the user already gave in the same flow, such as a network name or a path. |
| 3.3.8 Accessible Authentication (AA) | The lock screen, authentication agent (§18.1) and network-secret prompts accept pasted passwords and password managers, and never require a puzzle or memory test. |

**Classic density and target size.** Platinum Classic's reference geometry is denser than WCAG 2.2's 24×24 px minimum target: menu rows are 16 px tall, and the secondary button example is 80×16 px (§5). Classic 1× stays the reference geometry for conformance (§11). SLOPOS must also offer an accessible route: integer 2× scale (§9.2), which doubles every target, and a future **Large targets** preference that raises hit areas and row heights without changing glyph or bevel design. Whether Classic 1× meets EN 301 549 through an exception (spacing, equivalent control or essential presentation) is recorded per component in the dataset as `PASS`, `EXCEPTION` with a reason, or `FAIL`. It is never assumed.

**Other 2027 expectations:**

- **Color and contrast:** at least 4.5:1 for body text and 3:1 for large text, non-text UI parts and focus indicators (WCAG 1.4.3, 1.4.11). APCA may be used as an advisory second check only, because WCAG 3 is still a draft. Every style keeps a high-contrast variant or is labelled as not offering one.
- **Privacy indicators:** the menu bar shows when the camera, microphone, screen capture or the §21.6 operator is active. The indicator uses shape and text, not color alone, and no theme can hide it.
- **Text and language:** fonts are licensed and may be variable fonts. Color emoji, CJK input methods (IBus/Fcitx through §2.3's IME infrastructure) and right-to-left layouts are tested. In RTL, layout mirrors but icons with fixed meaning (play, clock) do not. Classic has no historical RTL reference, so the mirroring rules are written into the dataset as a SLOPOS extension (SPEC-02).
- **Motion:** reduced motion is honoured system-wide and by every style. Modern motion values come from its dataset (§5.2.6).
- **AI and Vision surfaces (§21.6, §21.7.3):** an operating indicator, a step list, a stop control and confirmations are always visible while the operator runs. Generated or recognised text is labelled as machine-generated and shows its confidence where meaningful. Changes are previewed and need review before they are applied, and they can be undone. No persona or anthropomorphic framing is required.
- **No dark patterns** (anti-pattern 23). Consent is opt-in, declining is as easy as accepting, and settings never reset silently.
