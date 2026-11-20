# Sunrise launcher: a map for agents

The GPUI launcher for [Sunrise](https://github.com/sunrise-org/sunrise): it installs, prepares and starts Sunrise
builds, and manages accounts, instances, mods, worlds and hosted servers, from its window, its CLI and its MCP server.
What players see: [README.md](README.md). Licence, DCO, the split from the game repository and its game copies:
[CONTRIBUTING.md](CONTRIBUTING.md).

## Rules that bite

- **Sign off every commit** (`git commit -s`): the DCO check (`tools/ci/dco_check.py`, `.github/workflows/dco.yml`)
  fails a PR with a commit that isn't. `git rebase --signoff origin/main` fixes a branch.
- **No hardcoded user-facing strings.** Wrap visible text in `tr("...")` or `trf("... {arg} ...", &[("arg", &val)])`
  from `crate::i18n` (`sunrise_launcher::i18n` in the binary's modules), never a raw literal or `format!`. The English
  is the key; translations are `lang/<code>.json`. `tools/lang_review/launcher/` records only text a person who speaks
  the language wrote or checked: never put machine translations there.
- **Keep your runs away from real player data.** Any launcher you start gets fresh `mktemp -d` folders in
  `SUNRISE_LAUNCHER_HOME`, `SUNRISE_HOME`, `XDG_DATA_HOME` and `XDG_CONFIG_HOME`, with `SUNRISE_LAUNCHER_TRASH`,
  `SUNRISE_LAUNCHER_CLIPBOARD` and `SUNRISE_LAUNCHER_DESKTOP` pointing inside that scratch home, plus
  `SUNRISE_LAUNCHER_NO_SELF_UPDATE=1` and `SUNRISE_NO_AUDIO=1` (ci.yml's "Keep the tests away" step sets the same).
  Never write to the real `~/.local/share/sunrise/` (`launcher`, `game`, `location`: `src/identity.rs`), the older
  `~/.local/share/sunrise-launcher` it carries over from, `~/.local/share/applications` or `~/.local/bin`.
- **Game copies stay in step.** `crates/` hold copies of game crates (and a few mirrored functions), checked against the
  Sunrise commit in `tools/ci/sunrise-commit` by `tools/tests/test_game_copies.py`. Bringing in a game change updates
  that pin and the copies in the same PR ([CONTRIBUTING.md](CONTRIBUTING.md#copies-of-game-files)).
- **Ignored tests:** the daily run (`.github/workflows/daily.yml`) runs every `#[ignore]`d test except those listed in
  `tools/ci/manual-tests.txt`. One that needs a person, a desktop, the network or a fixture process gets a row there
  (format in the file's header and rows: crate, test path, where it runs, `manual`, `# why`).
- **The CLI reference is generated.** `docs/launcher-commands.md` comes from `src/cli.rs` through `cli_tools::markdown()`;
  after changing a command, `SUNRISE_WRITE_DOCS=1 cargo test the_launcher_reference` rewrites it.
- **Mark what an agent posts.** A PR body starts with `> 🤖 Opened by <login>'s AI agent` and a blank line, where
  `<login>` is the account the PR is opened from (`gh api user --jq .login`). Issues say "Filed by", reviews
  "Review by". Every comment starts with `🤖 Agent:`. No `Co-Authored-By:` in commits, no "Generated with" in PRs.
- **Docs say where and how, not how many.** Point to the file and the command that lists a thing (`ls crates`,
  `ls tests`); no counts, timings or status that go stale.

## Don't lose time

- **Check nobody else is on it:** `gh pr list -R sunrise-org/sunrise-launcher --state open --search "<issue, file or function>"`.
- **Find code with the tables below** and the module's `//!` header (`grep -m1 '^//!' src/*.rs`), not by reading whole
  files. `src/backend.rs`, `src/cli.rs` and `src/native_mods.rs` are big: read the function you change and its callers.
- **Compile cheaply while you work:** `cargo check` after an edit; clippy and the full tests once, at the end.
- **Don't wait in loops:** start a long build or test in the background and carry on.

## Crates

The root crate `sunrise-launcher` is the launcher: a library (`src/lib.rs`, the core the GUI, the CLI and the MCP
server share) and the GUI binary (`src/main.rs`, which declares the window's modules). The workspace members are in the
root `Cargo.toml`; each crate's `description` says what it holds.

| Crate | Owns |
|---|---|
| `scenery` | The pixel scenes and plates: the sunset sky, rain, frosted glass. MIT |
| `crates/addresses` | Every outside address the launcher has built in |
| `crates/bench` | The benchmark report format, for the launcher's own measurements (`src/perf.rs`) |
| `crates/friends` | Play with Friends invite codes, read as the game reads them |
| `crates/mcp` | The Model Context Protocol transport behind `sunrise-launcher mcp` |
| `crates/nbt` | Java Edition NBT for `level.dat` and `servers.dat` |
| `crates/release-verify` | Signed release checksums, checked before an install (its `tests/sign_release.rs` checks `tools/ci/sign_release.sh` agrees) |
| `crates/resolver` | Minecraft SRV redirects, so the Servers list pings where the game connects |
| `crates/sdk` | A Sunrise SDK release: the signed `sdk.json` index and its verified slices, for native mods |
| `crates/shader-api` | Sunrise's own shader packs (`sunrise-shaders.toml`, `.sshader`) and their check |
| `crates/smod` | `.smod` packages and their manifest, modpacks, the registry index, a game run's shared files |
| `crates/system` | The machine the launcher runs on, for bug reports and telemetry |
| `crates/telemetry` | Opt-in data collection: consent, payloads, the outbox and its upload |
| `crates/theme` | Sunrise's design tokens, fonts and icons; `tokens.css` must match the game's |

Everything in `crates/` is a copy of a game crate: see the game-copies rule above.

## Where things live

Files are under `src/`. A `foo.rs` with a `foo/` folder has its submodules there. `*_page.rs` and `glass_*.rs` are the
window's views of the core module of the same name.

| Feature | Files |
|---|---|
| Window, frame, navigation | `main.rs` (`struct Launcher`), `launcher_setup.rs` (`Launcher::new`, page changes), `launcher_render.rs`, `launcher_messages.rs` + `wake.rs` (the backend's messages), `frame.rs`, `window_root.rs`, `shell.rs`, `rail.rs`, `titlebar.rs`, `status_strip.rs`, `command_search.rs` (Ctrl+K), `sheets.rs` |
| Core, background work | `backend.rs` + `backend/` (installs, preparation, the game's processes, release selection, messages); `process.rs`, `net.rs`, `proxy.rs`, `github.rs` |
| Accounts, Microsoft sign-in | `account.rs` + `account/` (device-code flow, Xbox, tokens, keychain or encrypted file, the owner gate), `microsoft.rs`, `private_window.rs`, `account_chip.rs`, `switch_page.rs`, `owner_page.rs`, `profile_dialog.rs`, `profile_heads.rs`, `developer_play.rs`, `developer_offline.rs`; [docs/microsoft-account.md](docs/microsoft-account.md) |
| Downloads and installs | `downloads.rs` + `downloads/`, `download_retry.rs`, `signed.rs`, `backend/installation.rs`, `backend/release_selection.rs`, `build_import.rs`, `developer_install.rs`, `nightly.rs`, `glass_versions.rs` |
| Preparing a build from Minecraft | `preparer.rs` (a version's `sunrise-prepare`), `sources.rs` (the player's own Minecraft), `mojang.rs`, `java.rs` + `java/`, `jdk.rs`, `jars.rs` + `jars/`, `jar_launch.rs` |
| Instances, worlds, packs | `instances.rs` + `instances/`, `instances_page.rs`, `glass_instance.rs`, `glass_sheet.rs`, `instance_icons.rs`, `worlds.rs` + `worlds_page.rs`, `datapacks.rs` + `datapacks_page.rs`, `packs.rs` + `packs_page.rs`, `screenshots.rs` + `screenshots_page.rs`, `export.rs`, `import.rs`, `zip_intake.rs`, `shortcut.rs` |
| Mods and the mod browser | `browser.rs` + `browser/` (the registry, Modrinth and other sources), `mods.rs` + `mods/`, `mods_page.rs` + `mods_page/`, `mods_project.rs`, `mod_icons.rs`, `home_mods.rs`, `instance_mods.rs`, `github_mods.rs` + `github_mods_page.rs` |
| Native mods | `native_mods.rs` + `native_mods/` (the build against the prebuilt SDK), `rust_toolchain.rs`, `native_fit.rs`, `native_status.rs`, `native_consent.rs`, `native_cache.rs`, `native_build.rs`, `native_log_page.rs`, `build_mode.rs`, `build.rs`, `blocks_settings.rs` (build consent) |
| Game launch, logs | `launch.rs` + `launch/` (what runs around the game), `launch_page.rs`, `launch_state.rs` (what Play says and does), `launcher_play.rs`, `while_playing.rs`, `game_app.rs` (macOS), `windows_runtime.rs`, `run_leases.rs`, `game_log.rs` + `game_log/`, `console_page.rs`, `bug_report.rs` |
| Servers and hosting | `servers.rs` + `servers/` (hosted `sunrise-server` folders), `servers_page.rs` + `servers_page/`, `glass_servers.rs`, `glass_server_dialog.rs`, `servers_format.rs`, `server_list.rs` + `multiplayer_page.rs` (`servers.dat`), `join.rs`, `heads.rs`, `internet.rs`, `tailscale.rs` |
| Settings and appearance | `settings.rs` + `settings/`, `appearance.rs`, `themes.rs`, `theme_files.rs`, `themes_page.rs`, `theme.rs`, `tokens.rs`, `paint.rs`, `motion.rs`, `blocks.rs`, `widgets.rs`, `glass_ui.rs`, `plate.rs`, `backdrop.rs`, `cat.rs`, `data_location.rs` + `location_page.rs` + `relocation.rs`, `storage.rs` + `glass_storage.rs`, `cache_cleanup.rs`, `collection.rs` + `telemetry.rs`, `no_ai.rs`; [docs/themes.md](docs/themes.md) |
| i18n | `i18n.rs` (`tr`, `trf`, language choice), `language_page.rs`, `lang/`, `tools/launcher_add_lang_keys.py`, `tools/lang_review/launcher/` |
| CLI and MCP | `cli.rs` + `cli/` (`sunrise-launcher <subcommand>`), `cli_tools.rs` (every command as an MCP tool), `mcp.rs`, `game_tools.rs` (the running game's tools), `console.rs`; [docs/launcher-commands.md](docs/launcher-commands.md), [docs/api/launcher.md](docs/api/launcher.md), `tools/launcher_mcp_e2e.py` |
| Updates and install on the desktop | `update_handoff.rs`, `update_recovery.rs`, `backend/update_selection.rs`, `mac_update.rs`, `carry_over.rs` (first start), `startup.rs`, `installer.rs` + `setup_page.rs` + `setup_args.rs` + `uninstall_aside.rs` (Windows), `desktop.rs` + `desktop_seams.rs` (Linux menu entry), `identity.rs`, `packaging/` |

Integration tests are in `tests/` and `crates/*/tests/` (`ls tests crates/*/tests`).

## Build and test

Linux needs GPUI's headers: `bash tools/ci/install_launcher_deps.sh libxkbcommon-x11-dev libwayland-dev libx11-xcb-dev
libfontconfig-dev libvulkan-dev libasound2-dev libudev-dev` (the list ci.yml passes). On NixOS, `nix-shell` in the
repository root (`shell.nix`). Rust is pinned in `rust-version` and `tools/ci/rust-version`.

What CI runs (`.github/workflows/ci.yml`), in the scratch homes above:

```sh
cargo fmt --all --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked --workspace --lib --bins
cargo test --locked --workspace --doc
cargo test --locked --workspace --test sign_release
cargo test --locked --test developer_install --test storage_consent
python3 -m unittest tools/tests/test_release_notes.py tools/tests/test_game_copies_issue.py tools/tests/test_rerun_unacquired.py
```

The game-copies check (needs a Sunrise checkout at the pin):

```sh
game=$(mktemp -d)
tools/ci/game_at.sh "$(cat tools/ci/sunrise-commit)" "$game"
SUNRISE_GAME_DIR="$game" python3 -m unittest tools/tests/test_game_copies.py
```

The `tests/` targets that start the launcher run in `daily.yml` (`processes` job), and the ignored tests in its
ignored-tests step: read it for the exact lines.
