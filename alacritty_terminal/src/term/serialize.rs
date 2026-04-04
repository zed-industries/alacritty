//! Terminal state snapshot and restore via serde.
//!
//! Captures the full terminal state — both grid buffers, cursor, mode flags,
//! and scroll region — as a [`TermState`] struct that can be serialised with
//! any serde-compatible format (bincode, JSON, …) and later applied back to
//! a [`Term`] via [`Term::restore`].
//!
//! [`Term`]: crate::Term

use std::ops::Range;

use serde::{Deserialize, Serialize};

use crate::grid::Grid;
use crate::index::Line;
use crate::term::cell::Cell;
use crate::term::TermMode;

/// Complete terminal state for serialisation and deserialisation.
///
/// Captures everything needed to restore a terminal session after reconnect:
/// both grid buffers (active and inactive, covering alternate screen), cursor
/// position and template, terminal mode flags, and scroll region.
///
/// # What is captured
///
/// | Field | Purpose |
/// |---|---|
/// | `grid` | Active grid with cursor, scrollback, cell content & attributes |
/// | `inactive_grid` | Inactive buffer (primary when alt screen is active, or vice versa) |
/// | `mode_bits` | `TermMode` bitflags: bracketed paste, mouse mode, alt screen, etc. |
/// | `scroll_region` | DECSTBM scroll region (top..bottom viewport lines) |
///
/// # What is NOT captured (defaults on restore)
///
/// - Character set mappings (`Charsets`) — almost never non-default
/// - Tab stops — reconstructed as standard 8-column stops
/// - Window title / title stack
/// - Keyboard mode stack
/// - Terminal colour overrides (come from client config)
/// - Cursor style (comes from client config)
/// - Selection state (client-side UI)
/// - Vi mode cursor (client-side UI)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TermState {
    /// Active grid (primary or alternate, depending on mode).
    pub grid: Grid<Cell>,

    /// Inactive grid (the other buffer).
    pub inactive_grid: Grid<Cell>,

    /// Terminal mode flags as raw `TermMode` bits.
    ///
    /// Stored as `u32` because `TermMode` (a `bitflags!` type) may not have
    /// serde derives. Use [`TermState::mode()`] to get the typed value.
    pub(crate) mode_bits: u32,

    /// Scroll region (top..bottom viewport lines).
    pub scroll_region: Range<Line>,
}

impl TermState {
    /// Terminal mode flags.
    pub fn mode(&self) -> TermMode {
        TermMode::from_bits_truncate(self.mode_bits)
    }
}

#[cfg(test)]
mod tests {
    use super::TermState;
    use crate::event::VoidListener;
    use crate::grid::{Dimensions, Grid};
    use crate::index::{Column, Line};
    use crate::term::cell::{Cell, Flags};
    use crate::term::test::TermSize;
    use crate::term::{Config, Term, TermMode};
    use crate::vte::ansi;

    /// Helper: create a term, push bytes through the parser, return the term.
    fn term_with(cols: usize, rows: usize, input: &[u8]) -> Term<VoidListener> {
        let size = TermSize::new(cols, rows);
        let mut term = Term::new(Config::default(), &size, VoidListener);
        let mut parser: ansi::Processor = ansi::Processor::new();
        parser.advance(&mut term, input);
        term
    }

    /// Helper: extract visible text (screen only, no scrollback) trimmed.
    fn visible_text(term: &Term<VoidListener>) -> String {
        let grid = term.grid();
        let mut lines = Vec::new();
        for row_idx in 0..grid.screen_lines() {
            let line = Line(row_idx as i32);
            let mut s = String::new();
            for col_idx in 0..grid.columns() {
                let col = Column(col_idx);
                let cell = &grid[line][col];
                if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                    continue;
                }
                s.push(cell.c);
            }
            lines.push(s.trim_end().to_string());
        }
        while lines.last().is_some_and(|l| l.is_empty()) {
            lines.pop();
        }
        lines.join("\n")
    }

    /// Helper: compare per-cell attributes for the visible screen area.
    fn assert_cells_equal(a: &Term<VoidListener>, b: &Term<VoidListener>) {
        let ga = a.grid();
        let gb = b.grid();
        assert_eq!(ga.screen_lines(), gb.screen_lines());
        assert_eq!(ga.columns(), gb.columns());

        for row_idx in 0..ga.screen_lines() {
            let line = Line(row_idx as i32);
            for col_idx in 0..ga.columns() {
                let col = Column(col_idx);
                let ca = &ga[line][col];
                let cb = &gb[line][col];
                assert_eq!(ca.c, cb.c, "char mismatch at ({row_idx}, {col_idx})");
                assert_eq!(ca.fg, cb.fg, "fg mismatch at ({row_idx}, {col_idx})");
                assert_eq!(ca.bg, cb.bg, "bg mismatch at ({row_idx}, {col_idx})");
                assert_eq!(ca.flags, cb.flags, "flags mismatch at ({row_idx}, {col_idx})");
            }
        }
    }

    #[test]
    fn grid_serde_round_trip() {
        let term = term_with(40, 10, b"\x1b[1;31mhello\x1b[0m world\r\nline two");
        let grid = term.grid();

        let json = serde_json::to_string(grid).expect("serialize Grid<Cell>");
        let grid2: Grid<Cell> = serde_json::from_str(&json).expect("deserialize Grid<Cell>");

        assert_eq!(grid.columns(), grid2.columns());
        assert_eq!(grid.screen_lines(), grid2.screen_lines());
        assert_eq!(grid.topmost_line(), grid2.topmost_line());

        for row_idx in 0..grid.screen_lines() {
            let line = Line(row_idx as i32);
            for col_idx in 0..grid.columns() {
                let col = Column(col_idx);
                let a = &grid[line][col];
                let b = &grid2[line][col];
                assert_eq!(a.c, b.c, "char mismatch at ({row_idx}, {col_idx})");
                assert_eq!(a.fg, b.fg, "fg mismatch at ({row_idx}, {col_idx})");
                assert_eq!(a.bg, b.bg, "bg mismatch at ({row_idx}, {col_idx})");
                assert_eq!(a.flags, b.flags, "flags mismatch at ({row_idx}, {col_idx})");
            }
        }
    }

    #[test]
    fn grid_serde_preserves_scrollback() {
        let term = term_with(
            40,
            4,
            b"line1\r\nline2\r\nline3\r\nline4\r\nline5\r\nline6\r\nline7\r\nline8",
        );
        let grid = term.grid();
        assert!(grid.topmost_line().0 < 0, "expected scrollback");

        let json = serde_json::to_string(grid).expect("serialize");
        let grid2: Grid<Cell> = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(grid.topmost_line(), grid2.topmost_line());

        for line_idx in grid.topmost_line().0..0 {
            let line = Line(line_idx);
            for col_idx in 0..grid.columns() {
                let col = Column(col_idx);
                assert_eq!(
                    grid[line][col].c,
                    grid2[line][col].c,
                    "scrollback mismatch at ({line_idx}, {col_idx})",
                );
            }
        }
    }

    #[test]
    fn grid_serde_preserves_wrapline() {
        let term = term_with(10, 4, b"abcdefghijklmno");
        let grid = term.grid();

        assert!(
            grid[Line(0)][Column(9)].flags.contains(Flags::WRAPLINE),
            "expected WRAPLINE on first row",
        );

        let json = serde_json::to_string(grid).expect("serialize");
        let grid2: Grid<Cell> = serde_json::from_str(&json).expect("deserialize");

        assert!(
            grid2[Line(0)][Column(9)].flags.contains(Flags::WRAPLINE),
            "WRAPLINE lost after serde round-trip",
        );
    }

    #[test]
    fn grid_serde_cursor_survives() {
        let term = term_with(40, 10, b"\x1b[1;31m\x1b[5;20Hhere");
        let grid = term.grid();
        assert_ne!(grid.cursor.point, Default::default());

        let json = serde_json::to_string(grid).expect("serialize");
        let grid2: Grid<Cell> = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(grid.cursor.point, grid2.cursor.point);
        assert_eq!(grid.cursor.template.fg, grid2.cursor.template.fg);
        assert_eq!(grid.cursor.template.bg, grid2.cursor.template.bg);
        assert_eq!(grid.cursor.template.flags, grid2.cursor.template.flags);
        assert_eq!(grid.cursor.input_needs_wrap, grid2.cursor.input_needs_wrap);
    }

    #[test]
    fn snapshot_restore_preserves_content_and_cursor() {
        let term1 = term_with(40, 10, b"\x1b[1;31mhello\x1b[0m world\r\n\x1b[5;20Hcursor here");
        let state = term1.snapshot();

        let size = TermSize::new(40, 10);
        let mut term2 = Term::new(Config::default(), &size, VoidListener);
        term2.restore(state);

        assert_eq!(visible_text(&term1), visible_text(&term2));
        assert_eq!(term1.grid().cursor.point, term2.grid().cursor.point);
        assert_cells_equal(&term1, &term2);
    }

    #[test]
    fn snapshot_restore_preserves_modes() {
        let term1 = term_with(40, 10, b"\x1b[?2004h\x1b[?1000hsome text");
        assert!(term1.mode().contains(TermMode::BRACKETED_PASTE));
        assert!(term1.mode().contains(TermMode::MOUSE_REPORT_CLICK));

        let state = term1.snapshot();
        let size = TermSize::new(40, 10);
        let mut term2 = Term::new(Config::default(), &size, VoidListener);
        term2.restore(state);

        assert_eq!(*term1.mode(), *term2.mode());
    }

    #[test]
    fn snapshot_restore_preserves_alternate_screen() {
        let mut input = Vec::new();
        input.extend_from_slice(b"primary line 1\r\nprimary line 2\r\n");
        input.extend_from_slice(b"\x1b[?1049h");
        input.extend_from_slice(b"alt screen content");

        let term1 = term_with(40, 10, &input);
        assert!(term1.mode().contains(TermMode::ALT_SCREEN));

        let state = term1.snapshot();
        let size = TermSize::new(40, 10);
        let mut term2 = Term::new(Config::default(), &size, VoidListener);
        term2.restore(state);

        assert!(term2.mode().contains(TermMode::ALT_SCREEN));
        assert_eq!(visible_text(&term1), visible_text(&term2));
    }

    #[test]
    fn snapshot_restore_preserves_scroll_region() {
        let term1 = term_with(40, 10, b"\x1b[3;8rtext after region set");
        let state1 = term1.snapshot();
        let scroll_region = state1.scroll_region.clone();
        assert_ne!(scroll_region, Line(0)..Line(10));

        let size = TermSize::new(40, 10);
        let mut term2 = Term::new(Config::default(), &size, VoidListener);
        term2.restore(state1);

        let state2 = term2.snapshot();
        assert_eq!(state2.scroll_region, scroll_region);
    }

    #[test]
    fn snapshot_restore_preserves_scrollback() {
        let term1 = term_with(
            40,
            4,
            b"line1\r\nline2\r\nline3\r\nline4\r\nline5\r\nline6\r\nline7\r\nline8",
        );
        assert!(term1.grid().topmost_line().0 < 0, "expected scrollback");

        let state = term1.snapshot();
        let size = TermSize::new(40, 4);
        let mut term2 = Term::new(Config::default(), &size, VoidListener);
        term2.restore(state);

        assert_eq!(term1.grid().topmost_line(), term2.grid().topmost_line());
        assert_eq!(visible_text(&term1), visible_text(&term2));

        for line_idx in term1.grid().topmost_line().0..0 {
            let line = Line(line_idx);
            for col_idx in 0..term1.grid().columns() {
                let col = Column(col_idx);
                assert_eq!(
                    term1.grid()[line][col].c,
                    term2.grid()[line][col].c,
                    "scrollback mismatch at ({line_idx}, {col_idx})",
                );
            }
        }
    }

    #[test]
    fn term_state_serde_json_round_trip() {
        let term = term_with(40, 10, b"\x1b[?2004h\x1b[1;31mhello\x1b[0m\x1b[5;20H");
        let state = term.snapshot();

        let json = serde_json::to_string(&state).expect("TermState JSON serialize");
        let state2: TermState = serde_json::from_str(&json).expect("TermState JSON deserialize");

        assert_eq!(state.grid.cursor.point, state2.grid.cursor.point);
        assert_eq!(state.mode(), state2.mode());
        assert_eq!(state.scroll_region, state2.scroll_region);
        assert_eq!(state.grid.columns(), state2.grid.columns());
        assert_eq!(state.grid.screen_lines(), state2.grid.screen_lines());
    }

    #[test]
    fn term_state_serde_bincode_round_trip() {
        let term = term_with(80, 24, b"\x1b[?2004h\x1b[?1000h\x1b[1;31mhello\x1b[0m world");
        let state = term.snapshot();

        let bytes = bincode::serde::encode_to_vec(&state, bincode::config::standard())
            .expect("TermState bincode serialize");
        let (state2, _): (TermState, _) =
            bincode::serde::decode_from_slice(&bytes, bincode::config::standard())
                .expect("TermState bincode deserialize");

        assert_eq!(state.grid.cursor.point, state2.grid.cursor.point);
        assert_eq!(state.mode(), state2.mode());
        assert_eq!(state.scroll_region, state2.scroll_region);
        assert_eq!(state.grid.columns(), state2.grid.columns());
    }
}
