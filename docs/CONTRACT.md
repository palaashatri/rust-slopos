# SLOPOS-I Engineering Contract

The long-form requirements, design rules and evidence ledger for SLOPOS-I, split into one file per area. Section numbers (§) are unchanged, so a reference like "§14.2" means section 14.2 in the file listed below. Older `qa/` data that cites `AGENTS.md §N` means the same section here.

The contract describes the **target**. For what is actually built today, read [`architecture.md`](architecture.md); for evidence of what was verified and when, read the [ledger](contract/ledger.md).

**Product:** SLOPOS-I / Platinum Classic, one monorepo on `main`
**Platform:** Linux, X11, Openbox, primarily Rust
**Visual authority:** Classic Macintosh UI Kit, Figma `LGMlwNCoVdakZxDBvPKg1W`, root `0:1` (clean-room, §5.1)
**Readiness:** alpha. Nothing here is a production-readiness claim.

Requirements describe the target; only evidence establishes implementation truth. The owner's current instructions take precedence over this contract.

| § | Section | File |
|---|---|---|
| §0 | Agent execution contract — read first | [`contract/00-agent-workflow.md`](contract/00-agent-workflow.md) |
| §1 | Product mission | [`contract/01-product.md`](contract/01-product.md) |
| §2 | Non-negotiable architectural principle | [`contract/01-product.md`](contract/01-product.md) |
| §3 | Scope and non-goals | [`contract/01-product.md`](contract/01-product.md) |
| §4 | Source-of-truth precedence | [`contract/01-product.md`](contract/01-product.md) |
| §5 | Canonical visual reference | [`contract/02-design.md`](contract/02-design.md) |
| §6 | High-level architecture | [`contract/03-architecture.md`](contract/03-architecture.md) |
| §7 | Target repository structure | [`contract/03-architecture.md`](contract/03-architecture.md) |
| §8 | slopos-core | [`contract/03-architecture.md`](contract/03-architecture.md) |
| §9 | slopos-ui — first-party component library | [`contract/04-toolkit.md`](contract/04-toolkit.md) |
| §10 | Figma-derived machine-readable design specification | [`contract/04-toolkit.md`](contract/04-toolkit.md) |
| §11 | slopos-ui conformance testing | [`contract/04-toolkit.md`](contract/04-toolkit.md) |
| §12 | slopos-appkit — native application framework | [`contract/04-toolkit.md`](contract/04-toolkit.md) |
| §13 | Platform integration libraries | [`contract/03-architecture.md`](contract/03-architecture.md) |
| §14 | SLOPOS shell | [`contract/05-shell.md`](contract/05-shell.md) |
| §15 | First-party application suite | [`contract/06-applications.md`](contract/06-applications.md) |
| §16 | Release-critical core applications | [`contract/06-applications.md`](contract/06-applications.md) |
| §17 | Extended first-party daily-use applications | [`contract/06-applications.md`](contract/06-applications.md) |
| §18 | System utilities and background infrastructure | [`contract/06-applications.md`](contract/06-applications.md) |
| §19 | Native application anatomy | [`contract/03-architecture.md`](contract/03-architecture.md) |
| §20 | Architectural enforcement | [`contract/03-architecture.md`](contract/03-architecture.md) |
| §21 | Dependency-driven implementation plan | [`contract/07-plan-and-vision.md`](contract/07-plan-and-vision.md) |
| §22 | System providers and truthful state | [`contract/08-system-integration.md`](contract/08-system-integration.md) |
| §23 | Window management and X11 | [`contract/08-system-integration.md`](contract/08-system-integration.md) |
| §24 | Multi-monitor and scaling | [`contract/08-system-integration.md`](contract/08-system-integration.md) |
| §25 | Accessibility | [`contract/08-system-integration.md`](contract/08-system-integration.md) |
| §26 | Application interoperability | [`contract/08-system-integration.md`](contract/08-system-integration.md) |
| §27 | Visual acceptance | [`contract/09-qa-and-release.md`](contract/09-qa-and-release.md) |
| §28 | Mandatory VM-only execution policy | [`contract/09-qa-and-release.md`](contract/09-qa-and-release.md) |
| §29 | Disk-space and resource safety | [`contract/09-qa-and-release.md`](contract/09-qa-and-release.md) |
| §30 | Verification commands and evidence | [`contract/09-qa-and-release.md`](contract/09-qa-and-release.md) |
| §31 | Required end-to-end daily-use journeys | [`contract/09-qa-and-release.md`](contract/09-qa-and-release.md) |
| §32 | Release engineering | [`contract/09-qa-and-release.md`](contract/09-qa-and-release.md) |
| §33 | No-cheating rules | [`contract/09-qa-and-release.md`](contract/09-qa-and-release.md) |
| §34 | Documentation and single-source truth | [`contract/09-qa-and-release.md`](contract/09-qa-and-release.md) |
| §35 | Definition of component completion | [`contract/09-qa-and-release.md`](contract/09-qa-and-release.md) |
| §36 | Definition of application completion | [`contract/09-qa-and-release.md`](contract/09-qa-and-release.md) |
| §37 | Definition of SLOPOS-I production readiness | [`contract/09-qa-and-release.md`](contract/09-qa-and-release.md) |
| Part II | Current audit and evidence ledger (Part II, sections A–N) | [`contract/ledger.md`](contract/ledger.md) |
