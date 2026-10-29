# rhwp (vendored)

- Source: https://github.com/edwardkim/rhwp tag v0.8.7 (commit 1a76570e833917d15817415a53c09ad61ab3203f)
- License: MIT (LICENSE)
- Copied: `src/` (library only), `crates/`, `build.rs`, `saved/blank2010.hwp`,
  `ttfs/opensource/NotoSansKR-Regular.ttf` (OFL), `assets/logo/logo-32.png`.
- Removed: the CLI binary and everything only it uses (`src/main.rs`, `src/cli/`,
  `src/mcp_serve.rs` — the only place that spawns processes —, `src/tools/`, `src/bin/`,
  and bin-only modules), tests/samples, workspace tooling.
- Cargo.toml trimmed to `[package]`, `[lib]` (rlib only), dependencies and features.
- Reviewed: the library has no network or process calls; `build.rs` only checks whether
  two optional source files exist.
