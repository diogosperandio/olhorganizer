# Context
This is Olhorganizer, a desktop app for non-technical users that watches folders
and moves or copies new files into destination folders built from markers such as
{YYYY}/{MM}. Example: NEWS/20261022.pdf -> D:\Jornais\2026\10\20261022.pdf.
Stack: Rust + Tauri 2, frontend Svelte + TypeScript + Vite (created with
create-tauri-app). Targets: Windows 10/11 and Linux (Ubuntu/Zorin/Mint, Fedora).
Identifier: com.diogosperandio.olhorganizer.
Cargo workspace: crates/motor (pure engine library), src-tauri (Tauri app), src/ (UI).

# Goal
Create CLAUDE.md at the project root so future sessions share the same context
and conventions.

# Requirements
CLAUDE.md must contain:
1. One-paragraph product summary (above).
2. Architecture: crates/motor = rules, markers, filter compiler, destination
   builder, file operations, no Tauri dependency. crates/monitor (added in step 4)
   = watching folders. src-tauri = commands, events, tray, plugins. src/ = UI only;
   the UI never touches files directly, it calls Tauri commands.
3. Marker syntax:
   - Date/name markers: {YYYY} {YY} {MM} {MES} (month name, pt-BR) {DD}
     {NOME} (file name without extension) {EXT}.
   - Extension lists in filters: .{pdf,jpg,png}. Matching is case-insensitive;
     jpg and jpeg are equivalent.
   - Wildcard: * (any text). Custom captures: {TEXTO:name} (letters/digits),
     {NUM:name} (digits), {*} (anything, not captured). A captured name becomes
     a marker usable in the destination as {name}.
   - The | character inside braces is reserved for future modifiers.
4. Conventions: Rust 2021, cargo fmt + clippy clean, errors with thiserror in
   libraries and anyhow only in binaries, no unwrap() outside tests, all
   user-facing strings in Brazilian Portuguese, code and comments in English.
5. Safety rules: never delete user files; cross-device moves = copy, verify size,
   then remove source; never overwrite unless the rule says so.
6. Commands to run: npm run tauri dev, cargo test --workspace,
   cargo clippy --workspace, cargo fmt --all.
7. A "Steps" checklist with steps 0-12 (titles only), step 0 checked.

# Constraints
- Keep CLAUDE.md under ~150 lines; it is read on every session.
- Do not change any other file.

# Deliverables
- CLAUDE.md

# Acceptance criteria
- All seven sections present and consistent with the rules above.

# Out of scope
- Any code.