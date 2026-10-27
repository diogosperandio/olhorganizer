# Olhorganizer

Olhorganizer is a desktop app for non-technical users that watches folders and moves or copies new files into destination folders built from markers such as `{YYYY}/{MM}`. Example: `NEWS/20261022.pdf -> D:\Jornais\2026\10\20261022.pdf`. The stack is Rust, Tauri 2, Svelte, TypeScript, and Vite; targets are Windows 10/11 and Linux (Ubuntu/Zorin/Mint and Fedora), and the identifier is `com.diogosperandio.olhorganizer-codex`.

## Architecture

- `crates/motor`: pure engine library for rules, markers, filter compilation, destination building, planning, and safe file operations. It must not depend on Tauri.
- `crates/monitor`: watches rule source folders, scans at intervals, waits for stable files, and invokes `motor`.
- `src-tauri`: Tauri commands, events, tray integration, and plugins.
- `src`: UI only. The UI never accesses files directly; it calls Tauri commands.

## Markers and filters

- Date/name markers: `{YYYY}`, `{YY}`, `{MM}`, `{MES}` (month name in pt-BR), `{DD}`, `{NOME}` (name without extension), and `{EXT}`.
- Extension lists use `.{pdf,jpg,png}`. Matching is case-insensitive, and `jpg`/`jpeg` are equivalent.
- `*` matches any text.
- `{TEXTO:name}` captures letters/digits, `{NUM:name}` captures digits, and `{*}` matches without capturing.
- A named capture becomes a destination marker such as `{name}`.
- `|` inside braces is reserved for future modifiers.

## Subfolders and destinations

- `include_subfolders: false` watches only files directly inside `source_dir`. The destination may be a subfolder of the source, such as `NEWS/{YYYY}/{MM}`, but it cannot equal `source_dir` itself.
- `include_subfolders: true` watches the complete source tree. The destination must be completely outside `source_dir`.
- Every destination must be absolute and begin with a fixed base folder before its first marker; the UI supplies that base through `Escolher pasta base`.
- Recursive scans never follow symbolic links and skip hidden folders, `$RECYCLE.BIN`, and `System Volume Information`.
- `Config::validate_all()` rejects cycles between enabled rules. A one-way chain remains valid but warns when one rule writes into another rule's watched area.
- As runtime defense, the monitor ignores its own file events for five seconds and pauses a rule that processes the same file identity more than three times in ten minutes. A paused rule requires a user action to resume.
- Filters always match the file name only, never the full or relative path.

## Conventions

- Rust 2021; keep `cargo fmt` and clippy clean.
- Libraries use `thiserror`; `anyhow` is allowed only in binaries.
- Do not use `unwrap()` outside tests.
- User-facing text is Brazilian Portuguese; code and comments are English.
- Rust DTOs exposed by Tauri and the hand-written interfaces in `src/lib/types.ts` must remain in sync.

## Safety

- Never delete user files except the verified source removal that completes a requested move.
- Cross-device moves copy, fsync, compare file size, and only then remove the source.
- Never overwrite unless the rule's conflict policy explicitly allows it.
- Never follow symbolic links out of a monitored source.
- Never delete source folders, including empty folders left after recursive processing.

## Commands

```sh
npm run tauri dev
cargo test --workspace
cargo clippy --workspace
cargo fmt --all
```

## Steps

- [x] Step 0 — Project context and conventions
- [x] Step 1 — Rule and configuration model
- [x] Step 2 — Marker registry and filter compiler
- [x] Step 3 — Destination planning and file operations
- [x] Step 4 — Folder monitor and CLI
- [x] Step 5 — Recursive folder monitoring
- [x] Step 6 — Application service integration
- [x] Step 7 — Rule editor UI
- [x] Step 8 — Activity and notifications
- [ ] Step 9 — Tray and startup integration
- [ ] Step 10 — Updates and packaging
- [ ] Step 11 — Platform validation
- [ ] Step 12 — Release readiness
