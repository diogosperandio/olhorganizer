# Olhorganizer

Desktop app for non-technical users that watches folders and moves or copies
new files into destination folders built from markers such as `{YYYY}/{MM}`.
Example: `NEWS/20261022.pdf` -> `D:\Jornais\2026\10\20261022.pdf`.
Stack: Rust + Tauri 2, frontend Svelte + TypeScript + Vite (create-tauri-app).
Targets: Windows 10/11 and Linux (Ubuntu/Zorin/Mint, Fedora).
Identifier: `com.diogosperandio.olhorganizer`.

## Architecture

Cargo workspace (root `Cargo.toml`; build output and `[profile.release]` live at the root).

- `crates/motor` — pure engine library, **no Tauri, notify or tokio**:
  - `rule.rs` / `config.rs`: `Rule`, `Config`, JSON load/save (atomic), validation.
  - `markers.rs`: marker registry (`list_markers()` feeds the UI buttons).
  - `filter.rs`: filter compiler -> `Filter::matches(file_name) -> Option<Captures>`.
  - `destination.rs`: expands the destination template.
  - `ops.rs`: `process(file, rule, dry_run) -> Outcome`; all disk access goes
    through the `FileSystem` trait (`StdFs` in production, fakes in tests).
  - `plan.rs`: `pick_rule` (which rule owns a file) and `test_rule` (dry run
    behind the "Testar regra" button).
- `crates/monitor` — watches folders (notify-debouncer-full + tokio), stable-file
  check, startup/interval/10-min rescans, pause/resume when disks disappear.
  Emits `MonitorEvent`. Binary `olhorganizer-cli` (feature `cli`, on by default;
  the Tauri app should depend with `default-features = false`).
- `src-tauri` — Tauri commands, events, tray, plugins. Calls motor/monitor.
- `src/` — UI only. The UI never touches files directly; it calls Tauri commands.

## Markers

- Date/name: `{YYYY}` `{YY}` `{MM}` `{MES}` (month name, pt-BR) `{DD}`
  `{NOME}` (file name without extension) `{EXT}`. `{NOME}`/`{EXT}` are
  destination-only.
- Extension lists in filters: `.{pdf,jpg,png}`. Case-insensitive; jpg = jpeg.
  Whole filter matching is case-insensitive.
- Wildcard: `*` (any text). Custom captures: `{TEXTO:name}` (letters/digits/-/_),
  `{NUM:name}` (digits), `{*}` (anything, not captured). A captured name becomes
  a destination marker `{name}`.
- `|` inside braces is reserved for future modifiers (error today).
- Filters match the **file name only**, never the path. Temporary names
  (`*.part`, `*.crdownload`, `*.tmp`, `~$*`) never match.
- Adding a marker = one entry in `markers.rs` + a test.

## Subfolders (`Rule.include_subfolders`)

`#[serde(default)]` false, so old config files load unchanged.

- **off** — only the top level of `source_dir` is watched. The destination may
  be a subfolder of it (`NEWS/{YYYY}/{MM}` is fine) but never `source_dir`
  itself: `RuleProblem::DestinationIsSource`.
- **on** — the whole tree is watched (recursive watcher, walkdir scans, no
  symlink following, hidden folders, `$RECYCLE.BIN` and
  `System Volume Information` skipped). The destination must be completely
  outside `source_dir`: `RuleProblem::DestinationInsideWatchedTree`.
- Every destination must be absolute and start with a fixed folder before any
  marker (`D:\Jornais\{YYYY}` ok; `{cliente}\{YYYY}`, `Jornais\{YYYY}` ->
  `RuleProblem::DestinationNeedsFixedFolder`, "...botão 'Escolher pasta base'").
- The checks are textual (fixed prefix before the first marker), compared by
  components after resolving `.`/`..` and canonicalizing existing parts;
  case-insensitive on Windows.
- A folder moved into a recursive source is scanned. Folders are never deleted.
- Linux inotify limit -> `RuleError` suggesting "A cada X minutos".

## Loop protection (three layers)

1. **Rule-cycle check** — `Config::validate_all() -> ConfigReport`. Edge A -> B
   when A's destination lies in B's watched area. Cycles (A <-> B,
   A -> B -> C -> A) are blocking errors ("As regras 'A' e 'B' moveriam os
   arquivos de uma para a outra sem parar."). Other edges are warnings ("Os
   arquivos movidos pela regra 'A' serão processados de novo pela regra 'B'.").
   `Config::save_rule` and `Config::import` run `Rule::validate` + this check
   and change nothing on failure. Every UI save/import path must go through them.
2. **Runtime guard** in `ops::process`: a file already in its destination
   folder, or below a destination that lies inside the source, is `Skipped`.
   Copy rules return `AlreadyCopied` (not re-copied, not reported) when an
   identical file (name, size, mtime) is already at the destination; copies
   keep the source's mtime.
3. **Circuit breaker** in `crates/monitor` (`breaker.rs`), for configs that
   bypassed validation (hand-edited config.json):
   - Watcher events for files the app wrote less than 5 s ago (same size and
     mtime) are ignored; scans still see them.
   - Per rule, a file identity (name without " (N)" counters + size + mtime,
     folder ignored) gets at most 3 moves/copies in 10 minutes. The next
     attempt pauses the rule with `RulePaused` ("Regra pausada: o mesmo
     arquivo foi processado várias vezes seguidas...").
   - It never deletes anything. Only `Monitor::resume(rule_id)` (a user
     action) or a changed rule restarts it.

## Conventions

- Rust edition: the conventions say 2021, but the crates currently use 2024.
  Pending decision; don't change it unasked.
- `cargo fmt` + `cargo clippy --workspace --all-targets` clean (no warnings).
- Errors: `thiserror` in libraries, `anyhow` only in binaries.
- No `unwrap()` outside tests.
- All user-facing strings in Brazilian Portuguese; code and comments in English.

## Safety rules

- Never delete user files (or folders).
- Cross-device move = copy to a temp name -> fsync -> verify size -> rename into
  place -> remove source. If any step fails, keep the source and report `Failed`.
- Never overwrite unless the rule's `on_conflict` is `Overwrite`
  (default `Rename`: `name (2).ext`, `name (3).ext`...).
- Dry run never creates, moves, copies or deletes anything.
- Never follow symlinks out of the source folder.

## Commands

```bash
npm run tauri dev                 # run the app
cargo test --workspace            # all tests
cargo clippy --workspace --all-targets
cargo fmt --all
cargo run -p monitor --bin olhorganizer-cli -- --config path/to/config.json [--dry-run]
```

`crates/motor/examples/config.example.json` is also a test fixture: its rules
must stay valid, or `cargo test -p motor` fails.

## Steps

- [x] 0. CLAUDE.md
- [x] 1. Rule and Config data model (JSON load/save, validation)
- [x] 2. Marker registry and filter compiler
- [x] 3. Destination builder, file operations, dry run (`test_rule`, `pick_rule`)
- [x] 4. Folder monitor (`crates/monitor`) and CLI
- [x] 5. Include subfolders per rule (4B: + cycle check, circuit breaker)
- [ ] 6. (title TBD)
- [ ] 7. UI: rule editor (adds the "Incluir subpastas" switch)
- [ ] 8. (title TBD)
- [ ] 9. (title TBD)
- [ ] 10. (title TBD)
- [ ] 11. (title TBD)
- [ ] 12. (title TBD)
