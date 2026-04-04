# Alacritty Terminal State Serialisation

Branch: `zed-pty`
Repo: `~/src/play/alacritty-serialisation`
Consumer: `~/src/play/zed-terminal-pty/crates/pty_host/`

---

## Context

Zed's `pty_host` crate runs a headless `Term<VoidListener>` in the shepherd
daemon to track terminal grid state. When a client reconnects after a Zed
restart, the grid state must be transferred to the client's real `Term` to
reconstruct the display — including scrollback, cell attributes, wrapped-line
state, and cursor position.

### Prior art

**VS Code / Kiro** use `@xterm/headless` (server-side JS terminal emulator) +
`@xterm/addon-serialize` (serialises the xterm.js buffer to ANSI escape
sequences). The serialised string is sent as `initialText` when reviving a
terminal process. Both sides are xterm.js, so the ANSI round-trip works. The
serialize addon is mature (~1,200 lines) and handles wrapping, terminal modes,
alternate screen, underline colour, scroll regions, and cursor SGR state.

**tmux** maintains a server-side virtual terminal and redraws the current
viewport on client attach. Scrollback lives in tmux's server, accessed via
tmux's own scroll commands — never replayed as ANSI.

### Our approach: direct serde of `Grid<Cell>`

Since both the pty_host (headless) and the Zed client use
`alacritty_terminal::Grid<Cell>`, we can skip the ANSI encode/decode round-trip
entirely and serialise the grid struct directly using serde.

**Why not ANSI?** An ANSI encoder must handle wrapping, SGR diffing, colour
models, cursor positioning, terminal modes, alternate screen, scroll regions,
and cursor SGR state. VS Code's addon-serialize does all of this, but it's
TypeScript tied to xterm.js. Reimplementing it in Rust is ~1,000+ lines of
terminal-specific code with an ongoing maintenance burden. The serde approach
gives lossless fidelity with zero custom serialisation logic.

**Why not xterm.js?** It's TypeScript, tied to xterm.js's buffer API. Zed's
terminal is Rust + alacritty_terminal. Using it would require embedding a JS
runtime or running a sidecar — disproportionate complexity for one feature.

**Size:** Binary serde (bincode) produces ~576 KB for an 80×24 terminal with
40 lines of content. This transfers over IPC in <1ms. For reconnect (a one-time
event), this is negligible.

---

## What serde gives us for free

The gaps identified in `gaps.md` (comparing our ANSI encoder against VS Code's
serialize addon) are largely **eliminated** by the serde approach, because we
serialise the raw struct rather than encoding semantics as escape sequences:

| Gap | ANSI approach | Serde approach |
|-----|---------------|----------------|
| 🔴 G1 — Terminal modes | Must emit DECSET/DECRST sequences | **Eliminated** — `TermMode` bitflags serialise directly |
| 🔴 G2 — Alternate screen buffer | Must serialise both buffers with `CSI ?1049h` switch | **Eliminated** — both `grid` and `inactive_grid` serialise directly |
| 🟡 G3 — Scroll region (DECSTBM) | Must emit `CSI Pt;Pb r` | **Eliminated** — `scroll_region` field serialises directly |
| 🟡 G4 — Wrapped lines | Must suppress `\r\n` and force-wrap; ~60 lines of edge cases | **Eliminated** — `WRAPLINE` flag on cells serialises directly |
| 🟡 G5 — Cursor SGR state | Must diff cursor template cell against reset state | **Eliminated** — `cursor.template` serialises directly |
| ⚪ G6 — ECH vs spaces | Cosmetic efficiency | **N/A** — no escape sequences emitted |
| ⚪ G7 — Overline / blink flags | Must add to attr mask + emit SGR 5/53 | **Eliminated** — all `Flags` bits serialise directly |
| ⚪ G8 — Underline colour | Must emit SGR 58:2/58:5 | **Eliminated** — `CellExtra` serialises directly |
| ⚪ G9 — Flag removal efficiency | Must implement individual SGR off-codes | **N/A** — no escape sequences emitted |

The only item that needs work is the **cursor**, which is currently
`#[serde(skip)]` on `Grid`. Everything else already round-trips through serde.

---

## Completed work

### ✅ T1 — ANSI `serialize_grid()` (proof of concept)

Implemented in `alacritty_terminal/src/term/serialize.rs`. Encodes grid state
as ANSI escape sequences with comprehensive spec references (ECMA-48, ITU-T
T.416, XTerm ctlseqs, kitty). Has 20 passing round-trip tests covering text,
colours, wide chars, combining chars, scrollback, underline variants, and
attribute diffing.

**Known limitations** (all eliminated by the serde approach):
- Soft-wrapped lines become hard newlines → broken selection/paste (G4)
- Terminal modes not preserved → bracketed paste safety issue (G1)
- Alternate screen not serialised → vim/less/htop lose everything (G2)
- Underline colour, overline, blink not emitted (G7, G8)
- Cursor SGR style reset to default (G5)

Kept as a reference and fallback. Module docs serve as the authoritative
escape sequence reference for the project.

### ✅ T2 — Serde proof of concept

`Grid<Cell>` already derives `Serialize`/`Deserialize` (behind the `serde`
feature flag, on by default). Round-trip tests pass with JSON, bincode, and
postcard. Cell content, attributes, colours, `WRAPLINE` flags, and scrollback
all survive losslessly. See `grid_serde_*` and `serde_vs_ansi_size_comparison`
tests in `serialize.rs`.

**Known issue:** `Grid.cursor` and `Grid.saved_cursor` are `#[serde(skip)]` —
they default to (0,0) after deserialise. This is the one thing that needs
fixing (T3 below).

---

## Remaining tasks

### T3. Include cursor in Grid serde

**Goal:** Cursor position and template cell survive serialisation.

`Grid<Cell>` currently has:
```rust
#[cfg_attr(feature = "serde", serde(skip))]
pub cursor: Cursor<T>,

#[cfg_attr(feature = "serde", serde(skip))]
pub saved_cursor: Cursor<T>,
```

The `Cursor<T>` struct doesn't derive `Serialize`/`Deserialize` because it
contains `Charsets` which wraps `[StandardCharset; 4]`, and `StandardCharset`
(defined in the `vte` crate) lacks serde derives.

**Approach (simplest):**
1. Add `#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]` to
   `Cursor<T>` in `alacritty_terminal/src/grid/mod.rs`
2. Mark `charsets: Charsets` as `#[cfg_attr(feature = "serde", serde(skip))]`
   — charsets default to ASCII on deserialise, which is fine (they're almost
   never non-default and not critical for reconnect)
3. Remove the `serde(skip)` from `cursor` and `saved_cursor` on `Grid`
4. Add/update round-trip test: verify cursor position and cursor template cell
   (fg, bg, flags) survive

**Files to modify:**
- `alacritty_terminal/src/grid/mod.rs` — `Cursor<T>` derive + `Grid` fields

**Verify:** `cargo test -p alacritty_terminal -- serialize`

### T4. Include terminal modes in serialisation

**Goal:** `TermMode` bitflags (bracketed paste, mouse tracking, etc.) survive
serialisation.

`TermMode` is a `bitflags` struct. With the `serde` feature enabled on
`bitflags` (already the case — see `Cargo.toml`), it should already derive
`Serialize`/`Deserialize`.

**Approach:**
1. Check whether `TermMode` already derives serde traits
2. Decide where modes live in the serialised payload — either:
   - Add a `serialize_state()` method on `Term<T>` that returns a struct
     containing the `Grid` + `TermMode` + any other top-level state
   - Or serialise `TermMode` as a sidecar alongside the grid bytes
3. On the client side, after deserialising the grid, apply the mode bitflags
   to the client's `Term`
4. Test: create a term with bracketed paste + mouse mode enabled, round-trip,
   verify modes survive

**Why this matters:** Without bracketed paste mode, pasting text after
reconnect is interpreted as raw keystrokes — potentially executing commands.
This is the highest-priority gap from `gaps.md` (G1).

### T5. Include alternate screen buffer

**Goal:** If the terminal is on the alternate screen (vim, less, htop), both
buffers survive serialisation.

`Term` stores the inactive buffer in `inactive_grid: Grid<Cell>`. Check:
- Does `Term` expose the inactive grid?
- Can we serialise a `TermSnapshot` struct containing both grids + modes?

**Approach:**
1. Define a serialisable struct:
   ```rust
   pub struct TermState {
       pub grid: Grid<Cell>,
       pub inactive_grid: Grid<Cell>,
       pub mode: TermMode,
       pub scroll_region: Range<Line>,
       // ... any other fields needed
   }
   ```
2. Add `Term::snapshot(&self) -> TermState` and
   `Term::restore(state: TermState)` (or equivalent)
3. The client deserialises `TermState` and swaps it into its `Term`
4. Test: enter alternate screen, write content, snapshot, restore, verify both
   buffers

### T6. Public API: `Term::to_bytes()` / `Term::from_bytes()`

**Goal:** Clean public API for the pty_host consumer.

```rust
impl<T> Term<T> {
    /// Serialise terminal state (grid, cursor, modes, alt screen) to bytes.
    pub fn to_bytes(&self) -> Vec<u8>;
}

impl Term<VoidListener> {
    /// Restore terminal state from bytes.
    pub fn from_bytes(bytes: &[u8], config: &Config) -> Result<Self, ...>;
}
```

Thin wrapper around serde + the chosen binary format (bincode recommended).
The derive does all the real work.

### T7. Integration: replace ANSI serialisation in pty_host

In `~/src/play/zed-terminal-pty/crates/pty_host/src/headless.rs`:

1. Replace `HeadlessTerminal::serialize_grid()` (~400 lines of hand-rolled
   ANSI emission) with `term.to_bytes()`
2. On the client side, replace `Processor::advance(&mut term, &serialised)`
   with `Term::from_bytes(&bytes)` and swap into the client
3. Delete all hand-rolled helpers: `emit_sgr_diff`, `emit_sgr_flags`,
   `emit_fg_color`, `emit_bg_color`, `named_color_to_sgr_*`, `write_sgr_*`,
   `write_cup`, `write_u32`

The pty_host's serialise/restore becomes two one-liners.

### T8. (Future) Login shell wrapping

`pty_host/src/session.rs` spawns the shell with a plain `Command::new(shell)`.
On macOS, alacritty's `tty/unix.rs` wraps this in
`/usr/bin/login -flp $USER /bin/zsh -fc "exec -a -zsh $SHELL"` for proper
session setup. Expose `default_shell_command()` or equivalent from
`alacritty_terminal`.

---

## Dev setup

```sh
# Run alacritty_terminal tests (including serde round-trip tests)
cargo test -p alacritty_terminal

# In the Zed repo — verify pty_host still compiles and its tests pass
cd ~/src/play/zed-terminal-pty
cargo test -p pty_host
```

The Zed workspace has a `[patch]` pointing `alacritty_terminal` at this local
checkout, so changes here are picked up automatically.

---

## Reference

- `gaps.md` — full gap analysis comparing our ANSI encoder vs VS Code's
  serialize addon (all gaps eliminated by the serde approach except cursor)
- `alacritty_terminal/src/term/serialize.rs` — ANSI encoder (T1) + serde
  proof-of-concept tests (T2) + escape sequence spec references
- VS Code pty host: `src/vs/platform/terminal/node/ptyService.ts` —
  `serializeTerminalState()`, `_reviveTerminalProcess()`,
  `generateReplayEvent()` with `@xterm/addon-serialize`
- VS Code serialize addon:
  [SerializeAddon.ts](https://github.com/xtermjs/xterm.js/blob/master/addons/addon-serialize/src/SerializeAddon.ts)