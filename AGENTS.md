# AGENTS.md

How the owner wants this project built. Follow this on every task.

## Communication

- Short and direct. No filler openers, no praise, no hedging paragraphs.
- Say what is verified and what is assumed, in plain words.
- Never claim something works unless it ran. "It compiles" is not "it works".

## Build

```bash
export PATH="$HOME/.cargo/bin:$PATH"
export PKG_CONFIG_PATH="$HOME/sysdeps/usr/lib/x86_64-linux-gnu/pkgconfig:$HOME/sysdeps/usr/share/pkgconfig"
export LIBRARY_PATH="$HOME/sysdeps/usr/lib/x86_64-linux-gnu"
export C_INCLUDE_PATH="$HOME/sysdeps/usr/include"
export RUSTFLAGS="-L $HOME/sysdeps/usr/lib/x86_64-linux-gnu"
```

No sudo on this machine. System headers live in `~/sysdeps` (extracted
`.deb` files). If a new system dependency is needed, download the `-dev`
package with `apt download` and extract it there, plus copy the runtime
`.so` files next to the symlinks or linking fails.

## Gates (run all four before every push)

```bash
cargo fmt && cargo fmt --check
cargo clippy -- -D warnings
cargo test
cargo run -- --offline-test
```

Check each gate on its own. Do not pipe through `tail` in a way that hides
a nonzero exit. A push with a red gate is a broken push.

## Code rules

- New behavior needs a test. Pure logic goes in unit tests, audio paths get
  covered by `--offline-test` (must stay non-silent and NaN-free).
- Prefer small structs over long argument lists. No `#[allow]` to silence
  design smells.
- Delete scratch files, probe examples and temp dirs before committing.
  No dead params, no no-op statements, no commented-out code.
- Cross-platform from the start: no `$HOME` assumptions (use `dirs`),
  no `/` path splits (use `Path::file_name`), test on the CI matrix
  (Linux, Windows, macOS), not just Linux.

## Verify, don't assume

- Installers get installed. Packages get run. Downloads get downloaded.
- When CI builds something you cannot run here (MSI, DMG), say exactly
  which part was verified and which part was only build-checked.
- If a fix needs a retry loop (tag, push, wait), keep the user posted
  instead of going silent through long sleeps.

## Writing (README, CHANGELOG, site, docs)

Researched AI tells and banned from all copy:

- No em dashes. Use commas, colons or periods.
- No performative honesty framing ("honest audit", "no excuses", "being straight").
- No hype adjectives: seamless, robust, cutting-edge, game-changer, vibrant,
  crucial, pivotal, ultimate, premium.
- No banned verbs/nouns: delve, leverage, harness, foster, showcase, embark,
  tapestry, landscape (figurative), realm, testament.
- No emoji headers. No "in conclusion" wrappers. Just end.
- One "robust" is a warning sign. Rewrite the sentence.

Site copy targets musicians, not developers. No architecture tables, no CI
talk, no module lists on the website. Repo docs can carry the technical detail.

## Git and releases

- Logical commits with specific messages, never "fix stuff" or "updates".
  Keep history clean enough to read top to bottom.
- Push when done. The user checks. `main` and `origin/main` must match
  before reporting completion.
- Releases use cargo-dist. Flow: bump version + CHANGELOG, commit, push,
  tag `vX.Y.Z`, push tag, watch the Release run, then the package run.
- Retagging a failed release is allowed only when nothing was published.
  Delete the remote tag first, then recreate it.
- `dist generate` output (`release.yml`, `wix/main.wxs`) is committed.
  Hand edits to `main.wxs` are protected by `allow-dirty = ["msi"]`.
- After release: confirm artifacts exist, test what can be tested here
  (installer script, AppImage), update the Download page.

## Research first

For unfamiliar domains (DAW design, installer tooling, style rules), search
before implementing. Cite what the research changed in the final summary.
