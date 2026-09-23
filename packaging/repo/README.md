# SLOPOS-I Package Repository Status

SLOPOS-I does **not currently publish a public official APT or Pacman repository**.

This directory contains repository-generation and release-engineering work only. It must not be interpreted as proof that a public package source exists.

In particular, do not publish or document enrollment commands using placeholder domains, keys or repository metadata.

## Current user installation paths

At the current alpha stage:

- source installation exists for development/testing;
- package artifacts may be produced by release workflows;
- a downloadable package artifact is not the same as a supported public package repository;
- no public SLOPOS APT/Pacman enrollment command should be advertised until the publication gates below pass.

## Publication requirements

A public package repository may be documented as available only when all of the following are true for the exact release channel:

1. repository metadata is generated from CI-built package artifacts;
2. metadata and packages are signed with the real release key;
3. the public HTTPS endpoint exists and is reachable;
4. the documented signing-key fingerprint matches the published key;
5. installation is tested from a clean Linux VM using only the public endpoint;
6. update/upgrade from the previous supported release is tested;
7. removal is tested;
8. the installed package starts a usable SLOPOS X11 session;
9. checksums/provenance are tied to the exact release commit;
10. README/TRUTH documentation is updated only after those checks pass.

Until then, repository publication status is:

**NOT PUBLISHED**

## Local/testing repositories

Release engineering may create temporary local APT/Pacman repositories inside dedicated Linux VMs for testing repository metadata, signatures and installation flows.

Test keys and local endpoints must be clearly marked as ephemeral and must never be copied into user-facing enrollment documentation.

All local repository generation, package installation and validation must follow the VM-only execution and disk-space policies in the root [AGENTS.md](../../AGENTS.md).
