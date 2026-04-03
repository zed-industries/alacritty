//! Tests for the Grid.

use super::*;

use crate::term::cell::Cell;
use crate::vte::ansi::{Color, NamedColor};

impl GridCell for usize {
    fn is_empty(&self) -> bool {
        *self == 0
    }

    fn reset(&mut self, template: &Self) {
        *self = *template;
    }

    fn flags(&self) -> &Flags {
        unimplemented!();
    }

    fn flags_mut(&mut self) -> &mut Flags {
        unimplemented!();
    }
}

// Scroll up moves lines upward.
#[test]
fn scroll_up() {
    let mut grid = Grid::<usize>::new(10, 1, 0);
    for i in 0..10 {
        grid[Line(i as i32)][Column(0)] = i;
    }

    grid.scroll_up::<usize>(&(Line(0)..Line(10)), 2);

    assert_eq!(grid[Line(0)][Column(0)], 2);
    assert_eq!(grid[Line(0)].occ, 1);
    assert_eq!(grid[Line(1)][Column(0)], 3);
    assert_eq!(grid[Line(1)].occ, 1);
    assert_eq!(grid[Line(2)][Column(0)], 4);
    assert_eq!(grid[Line(2)].occ, 1);
    assert_eq!(grid[Line(3)][Column(0)], 5);
    assert_eq!(grid[Line(3)].occ, 1);
    assert_eq!(grid[Line(4)][Column(0)], 6);
    assert_eq!(grid[Line(4)].occ, 1);
    assert_eq!(grid[Line(5)][Column(0)], 7);
    assert_eq!(grid[Line(5)].occ, 1);
    assert_eq!(grid[Line(6)][Column(0)], 8);
    assert_eq!(grid[Line(6)].occ, 1);
    assert_eq!(grid[Line(7)][Column(0)], 9);
    assert_eq!(grid[Line(7)].occ, 1);
    assert_eq!(grid[Line(8)][Column(0)], 0); // was 0.
    assert_eq!(grid[Line(8)].occ, 0);
    assert_eq!(grid[Line(9)][Column(0)], 0); // was 1.
    assert_eq!(grid[Line(9)].occ, 0);
}

// Scroll down moves lines downward.
#[test]
fn scroll_down() {
    let mut grid = Grid::<usize>::new(10, 1, 0);
    for i in 0..10 {
        grid[Line(i as i32)][Column(0)] = i;
    }

    grid.scroll_down::<usize>(&(Line(0)..Line(10)), 2);

    assert_eq!(grid[Line(0)][Column(0)], 0); // was 8.
    assert_eq!(grid[Line(0)].occ, 0);
    assert_eq!(grid[Line(1)][Column(0)], 0); // was 9.
    assert_eq!(grid[Line(1)].occ, 0);
    assert_eq!(grid[Line(2)][Column(0)], 0);
    assert_eq!(grid[Line(2)].occ, 1);
    assert_eq!(grid[Line(3)][Column(0)], 1);
    assert_eq!(grid[Line(3)].occ, 1);
    assert_eq!(grid[Line(4)][Column(0)], 2);
    assert_eq!(grid[Line(4)].occ, 1);
    assert_eq!(grid[Line(5)][Column(0)], 3);
    assert_eq!(grid[Line(5)].occ, 1);
    assert_eq!(grid[Line(6)][Column(0)], 4);
    assert_eq!(grid[Line(6)].occ, 1);
    assert_eq!(grid[Line(7)][Column(0)], 5);
    assert_eq!(grid[Line(7)].occ, 1);
    assert_eq!(grid[Line(8)][Column(0)], 6);
    assert_eq!(grid[Line(8)].occ, 1);
    assert_eq!(grid[Line(9)][Column(0)], 7);
    assert_eq!(grid[Line(9)].occ, 1);
}

#[test]
fn scroll_down_with_history() {
    let mut grid = Grid::<usize>::new(10, 1, 1);
    grid.increase_scroll_limit(1);
    for i in 0..10 {
        grid[Line(i as i32)][Column(0)] = i;
    }

    grid.scroll_down::<usize>(&(Line(0)..Line(10)), 2);

    assert_eq!(grid[Line(0)][Column(0)], 0); // was 8.
    assert_eq!(grid[Line(0)].occ, 0);
    assert_eq!(grid[Line(1)][Column(0)], 0); // was 9.
    assert_eq!(grid[Line(1)].occ, 0);
    assert_eq!(grid[Line(2)][Column(0)], 0);
    assert_eq!(grid[Line(2)].occ, 1);
    assert_eq!(grid[Line(3)][Column(0)], 1);
    assert_eq!(grid[Line(3)].occ, 1);
    assert_eq!(grid[Line(4)][Column(0)], 2);
    assert_eq!(grid[Line(4)].occ, 1);
    assert_eq!(grid[Line(5)][Column(0)], 3);
    assert_eq!(grid[Line(5)].occ, 1);
    assert_eq!(grid[Line(6)][Column(0)], 4);
    assert_eq!(grid[Line(6)].occ, 1);
    assert_eq!(grid[Line(7)][Column(0)], 5);
    assert_eq!(grid[Line(7)].occ, 1);
    assert_eq!(grid[Line(8)][Column(0)], 6);
    assert_eq!(grid[Line(8)].occ, 1);
    assert_eq!(grid[Line(9)][Column(0)], 7);
    assert_eq!(grid[Line(9)].occ, 1);
}

// Test that GridIterator works.
#[test]
fn test_iter() {
    let assert_indexed = |value: usize, indexed: Option<Indexed<&usize>>| {
        assert_eq!(Some(&value), indexed.map(|indexed| indexed.cell));
    };

    let mut grid = Grid::<usize>::new(5, 5, 0);
    for i in 0..5 {
        for j in 0..5 {
            grid[Line(i)][Column(j)] = i as usize * 5 + j;
        }
    }

    let mut iter = grid.iter_from(Point::new(Line(0), Column(0)));

    assert_eq!(None, iter.prev());
    assert_indexed(1, iter.next());
    assert_eq!(Column(1), iter.point().column);
    assert_eq!(0, iter.point().line);

    assert_indexed(2, iter.next());
    assert_indexed(3, iter.next());
    assert_indexed(4, iter.next());

    // Test line-wrapping.
    assert_indexed(5, iter.next());
    assert_eq!(Column(0), iter.point().column);
    assert_eq!(1, iter.point().line);

    assert_indexed(4, iter.prev());
    assert_eq!(Column(4), iter.point().column);
    assert_eq!(0, iter.point().line);

    // Make sure iter.cell() returns the current iterator position.
    assert_eq!(&4, iter.cell());

    // Test that iter ends at end of grid.
    let mut final_iter = grid.iter_from(Point { line: Line(4), column: Column(4) });
    assert_eq!(None, final_iter.next());
    assert_indexed(23, final_iter.prev());
}

#[test]
fn shrink_reflow() {
    let mut grid = Grid::<Cell>::new(1, 5, 2);
    grid[Line(0)][Column(0)] = cell('1');
    grid[Line(0)][Column(1)] = cell('2');
    grid[Line(0)][Column(2)] = cell('3');
    grid[Line(0)][Column(3)] = cell('4');
    grid[Line(0)][Column(4)] = cell('5');

    grid.resize(true, 1, 2);

    assert_eq!(grid.total_lines(), 3);

    assert_eq!(grid[Line(-2)].len(), 2);
    assert_eq!(grid[Line(-2)][Column(0)], cell('1'));
    assert_eq!(grid[Line(-2)][Column(1)], wrap_cell('2'));

    assert_eq!(grid[Line(-1)].len(), 2);
    assert_eq!(grid[Line(-1)][Column(0)], cell('3'));
    assert_eq!(grid[Line(-1)][Column(1)], wrap_cell('4'));

    assert_eq!(grid[Line(0)].len(), 2);
    assert_eq!(grid[Line(0)][Column(0)], cell('5'));
    assert_eq!(grid[Line(0)][Column(1)], Cell::default());
}

#[test]
fn shrink_reflow_twice() {
    let mut grid = Grid::<Cell>::new(1, 5, 2);
    grid[Line(0)][Column(0)] = cell('1');
    grid[Line(0)][Column(1)] = cell('2');
    grid[Line(0)][Column(2)] = cell('3');
    grid[Line(0)][Column(3)] = cell('4');
    grid[Line(0)][Column(4)] = cell('5');

    grid.resize(true, 1, 4);
    grid.resize(true, 1, 2);

    assert_eq!(grid.total_lines(), 3);

    assert_eq!(grid[Line(-2)].len(), 2);
    assert_eq!(grid[Line(-2)][Column(0)], cell('1'));
    assert_eq!(grid[Line(-2)][Column(1)], wrap_cell('2'));

    assert_eq!(grid[Line(-1)].len(), 2);
    assert_eq!(grid[Line(-1)][Column(0)], cell('3'));
    assert_eq!(grid[Line(-1)][Column(1)], wrap_cell('4'));

    assert_eq!(grid[Line(0)].len(), 2);
    assert_eq!(grid[Line(0)][Column(0)], cell('5'));
    assert_eq!(grid[Line(0)][Column(1)], Cell::default());
}

#[test]
fn shrink_reflow_empty_cell_inside_line() {
    let mut grid = Grid::<Cell>::new(1, 5, 3);
    grid[Line(0)][Column(0)] = cell('1');
    grid[Line(0)][Column(1)] = Cell::default();
    grid[Line(0)][Column(2)] = cell('3');
    grid[Line(0)][Column(3)] = cell('4');
    grid[Line(0)][Column(4)] = Cell::default();

    grid.resize(true, 1, 2);

    assert_eq!(grid.total_lines(), 2);

    assert_eq!(grid[Line(-1)].len(), 2);
    assert_eq!(grid[Line(-1)][Column(0)], cell('1'));
    assert_eq!(grid[Line(-1)][Column(1)], wrap_cell(' '));

    assert_eq!(grid[Line(0)].len(), 2);
    assert_eq!(grid[Line(0)][Column(0)], cell('3'));
    assert_eq!(grid[Line(0)][Column(1)], cell('4'));

    grid.resize(true, 1, 1);

    assert_eq!(grid.total_lines(), 4);

    assert_eq!(grid[Line(-3)].len(), 1);
    assert_eq!(grid[Line(-3)][Column(0)], wrap_cell('1'));

    assert_eq!(grid[Line(-2)].len(), 1);
    assert_eq!(grid[Line(-2)][Column(0)], wrap_cell(' '));

    assert_eq!(grid[Line(-1)].len(), 1);
    assert_eq!(grid[Line(-1)][Column(0)], wrap_cell('3'));

    assert_eq!(grid[Line(0)].len(), 1);
    assert_eq!(grid[Line(0)][Column(0)], cell('4'));
}

#[test]
fn grow_reflow() {
    let mut grid = Grid::<Cell>::new(2, 2, 0);
    grid[Line(0)][Column(0)] = cell('1');
    grid[Line(0)][Column(1)] = wrap_cell('2');
    grid[Line(1)][Column(0)] = cell('3');
    grid[Line(1)][Column(1)] = Cell::default();

    grid.resize(true, 2, 3);

    assert_eq!(grid.total_lines(), 2);

    assert_eq!(grid[Line(0)].len(), 3);
    assert_eq!(grid[Line(0)][Column(0)], cell('1'));
    assert_eq!(grid[Line(0)][Column(1)], cell('2'));
    assert_eq!(grid[Line(0)][Column(2)], cell('3'));

    // Make sure rest of grid is empty.
    assert_eq!(grid[Line(1)].len(), 3);
    assert_eq!(grid[Line(1)][Column(0)], Cell::default());
    assert_eq!(grid[Line(1)][Column(1)], Cell::default());
    assert_eq!(grid[Line(1)][Column(2)], Cell::default());
}

#[test]
fn grow_reflow_multiline() {
    let mut grid = Grid::<Cell>::new(3, 2, 0);
    grid[Line(0)][Column(0)] = cell('1');
    grid[Line(0)][Column(1)] = wrap_cell('2');
    grid[Line(1)][Column(0)] = cell('3');
    grid[Line(1)][Column(1)] = wrap_cell('4');
    grid[Line(2)][Column(0)] = cell('5');
    grid[Line(2)][Column(1)] = cell('6');

    grid.resize(true, 3, 6);

    assert_eq!(grid.total_lines(), 3);

    assert_eq!(grid[Line(0)].len(), 6);
    assert_eq!(grid[Line(0)][Column(0)], cell('1'));
    assert_eq!(grid[Line(0)][Column(1)], cell('2'));
    assert_eq!(grid[Line(0)][Column(2)], cell('3'));
    assert_eq!(grid[Line(0)][Column(3)], cell('4'));
    assert_eq!(grid[Line(0)][Column(4)], cell('5'));
    assert_eq!(grid[Line(0)][Column(5)], cell('6'));

    // Make sure rest of grid is empty.
    for r in (1..3).map(Line::from) {
        assert_eq!(grid[r].len(), 6);
        for c in 0..6 {
            assert_eq!(grid[r][Column(c)], Cell::default());
        }
    }
}

#[test]
fn grow_reflow_disabled() {
    let mut grid = Grid::<Cell>::new(2, 2, 0);
    grid[Line(0)][Column(0)] = cell('1');
    grid[Line(0)][Column(1)] = wrap_cell('2');
    grid[Line(1)][Column(0)] = cell('3');
    grid[Line(1)][Column(1)] = Cell::default();

    grid.resize(false, 2, 3);

    assert_eq!(grid.total_lines(), 2);

    assert_eq!(grid[Line(0)].len(), 3);
    assert_eq!(grid[Line(0)][Column(0)], cell('1'));
    assert_eq!(grid[Line(0)][Column(1)], wrap_cell('2'));
    assert_eq!(grid[Line(0)][Column(2)], Cell::default());

    assert_eq!(grid[Line(1)].len(), 3);
    assert_eq!(grid[Line(1)][Column(0)], cell('3'));
    assert_eq!(grid[Line(1)][Column(1)], Cell::default());
    assert_eq!(grid[Line(1)][Column(2)], Cell::default());
}

#[test]
fn shrink_reflow_disabled() {
    let mut grid = Grid::<Cell>::new(1, 5, 2);
    grid[Line(0)][Column(0)] = cell('1');
    grid[Line(0)][Column(1)] = cell('2');
    grid[Line(0)][Column(2)] = cell('3');
    grid[Line(0)][Column(3)] = cell('4');
    grid[Line(0)][Column(4)] = cell('5');

    grid.resize(false, 1, 2);

    assert_eq!(grid.total_lines(), 1);

    assert_eq!(grid[Line(0)].len(), 2);
    assert_eq!(grid[Line(0)][Column(0)], cell('1'));
    assert_eq!(grid[Line(0)][Column(1)], cell('2'));
}

#[test]
fn accurate_size_hint() {
    let grid = Grid::<Cell>::new(5, 5, 2);

    size_hint_matches_count(grid.iter_from(Point::new(Line(0), Column(0))));
    size_hint_matches_count(grid.iter_from(Point::new(Line(2), Column(3))));
    size_hint_matches_count(grid.iter_from(Point::new(Line(4), Column(4))));
    size_hint_matches_count(grid.iter_from(Point::new(Line(4), Column(2))));
    size_hint_matches_count(grid.iter_from(Point::new(Line(10), Column(10))));
    size_hint_matches_count(grid.iter_from(Point::new(Line(2), Column(10))));

    let mut iterator = grid.iter_from(Point::new(Line(3), Column(1)));
    iterator.next();
    iterator.next();
    size_hint_matches_count(iterator);

    size_hint_matches_count(grid.display_iter());
}

fn size_hint_matches_count<T>(iter: impl Iterator<Item = T>) {
    let iterator = iter.into_iter();
    let (lower, upper) = iterator.size_hint();
    let count = iterator.count();
    assert_eq!(lower, count);
    assert_eq!(upper, Some(count));
}

// https://github.com/rust-lang/rust-clippy/pull/6375
#[allow(clippy::all)]
fn cell(c: char) -> Cell {
    let mut cell = Cell::default();
    cell.c = c;
    cell
}

fn wrap_cell(c: char) -> Cell {
    let mut cell = cell(c);
    cell.flags.insert(Flags::WRAPLINE);
    cell
}

fn colored_cell(c: char, fg: Color, bg: Color) -> Cell {
    Cell { c, fg, bg, ..Cell::default() }
}

// ---------------------------------------------------------------------------
// Scrollback compression integration tests
// ---------------------------------------------------------------------------

/// Build a grid with `history_lines` of scrollback, each containing
/// identifiable content based on the line number.
fn grid_with_scrollback(
    screen_lines: usize,
    columns: usize,
    history_lines: usize,
) -> Grid<Cell> {
    let mut grid = Grid::<Cell>::new(screen_lines, columns, history_lines);

    let colours = [
        Color::Named(NamedColor::Red),
        Color::Named(NamedColor::Green),
        Color::Named(NamedColor::Blue),
        Color::Named(NamedColor::Yellow),
        Color::Named(NamedColor::Cyan),
    ];
    let default_bg = Color::Named(NamedColor::Background);

    // Scroll up `history_lines` times, filling each line with content.
    for n in 0..history_lines {
        // Write identifiable content into the top visible line before scrolling.
        let text_len = 5 + (n % (columns - 5));
        let fg = colours[n % colours.len()];
        for col in 0..text_len {
            let c = (b'!' + ((n + col) % 94) as u8) as char;
            grid[Line(0)][Column(col)] = colored_cell(c, fg, default_bg);
        }
        if n % 3 == 0 {
            grid[Line(0)][Column(columns - 1)].flags.insert(Flags::WRAPLINE);
        }

        grid.scroll_up::<Color>(&(Line(0)..Line(screen_lines as i32)), 1);
    }

    grid
}

/// Read all history rows as owned copies for later comparison.
fn snapshot_history(grid: &Grid<Cell>, columns: usize) -> Vec<Vec<Cell>> {
    let history = grid.history_size();
    let mut rows = Vec::with_capacity(history);
    for i in 0..history {
        let line = Line(-((history - i) as i32)); // oldest first
        let mut cells = Vec::with_capacity(columns);
        for col in 0..columns {
            cells.push(grid[line][Column(col)].clone());
        }
        rows.push(cells);
    }
    rows
}

#[test]
fn compress_and_thaw_round_trips_all_rows() {
    let screen = 10;
    let columns = 80;
    let history = 200;

    let mut grid = grid_with_scrollback(screen, columns, history);
    assert_eq!(grid.history_size(), history);

    // Snapshot the history before compression.
    let before = snapshot_history(&grid, columns);

    // Compress all but 20 hot rows.
    let keep_hot = 20;
    grid.compress_old_scrollback(keep_hot);

    assert_eq!(grid.history_size(), keep_hot);
    assert_eq!(grid.compressed_history_len(), history - keep_hot);
    assert_eq!(grid.total_history_size(), history);

    // Thaw everything back.
    grid.thaw_compressed_history(history - keep_hot);

    assert_eq!(grid.history_size(), history);
    assert_eq!(grid.compressed_history_len(), 0);

    // Verify every row matches the original.
    let after = snapshot_history(&grid, columns);
    for (row_idx, (orig, restored)) in before.iter().zip(after.iter()).enumerate() {
        for (col, (o, r)) in orig.iter().zip(restored.iter()).enumerate() {
            assert_eq!(o, r, "Mismatch at history row {row_idx}, column {col}");
        }
    }
}

#[test]
fn compress_reduces_memory() {
    let screen = 24;
    let columns = 160;
    let history = 5000;

    let mut grid = grid_with_scrollback(screen, columns, history);

    let dense_bytes = grid.history_size() * super::compact::dense_row_bytes(columns);

    grid.compress_old_scrollback(100);

    let compressed_bytes = grid.compressed_history_bytes();
    let ratio = dense_bytes as f64 / compressed_bytes as f64;

    assert!(
        ratio > 3.0,
        "Expected >3× compression, got {ratio:.1}× (dense={dense_bytes}, compressed={compressed_bytes})"
    );
}

#[test]
fn compress_no_op_when_history_below_threshold() {
    let mut grid = grid_with_scrollback(10, 40, 50);

    grid.compress_old_scrollback(100);

    assert_eq!(grid.history_size(), 50);
    assert_eq!(grid.compressed_history_len(), 0);
}

#[test]
fn compress_all_then_thaw_all() {
    let screen = 5;
    let columns = 20;
    let history = 30;

    let mut grid = grid_with_scrollback(screen, columns, history);
    let before = snapshot_history(&grid, columns);

    // Compress everything (keep_hot = 0).
    grid.compress_old_scrollback(0);
    assert_eq!(grid.history_size(), 0);
    assert_eq!(grid.compressed_history_len(), history);

    // Thaw everything.
    grid.thaw_compressed_history(history);
    assert_eq!(grid.history_size(), history);
    assert_eq!(grid.compressed_history_len(), 0);

    let after = snapshot_history(&grid, columns);
    assert_eq!(before, after);
}

#[test]
fn incremental_thaw() {
    let screen = 5;
    let columns = 20;
    let history = 100;

    let mut grid = grid_with_scrollback(screen, columns, history);
    let before = snapshot_history(&grid, columns);

    grid.compress_old_scrollback(10);
    assert_eq!(grid.compressed_history_len(), 90);

    // Thaw in batches.
    grid.thaw_compressed_history(30);
    assert_eq!(grid.history_size(), 40);
    assert_eq!(grid.compressed_history_len(), 60);

    grid.thaw_compressed_history(60);
    assert_eq!(grid.history_size(), 100);
    assert_eq!(grid.compressed_history_len(), 0);

    let after = snapshot_history(&grid, columns);
    assert_eq!(before, after);
}

#[test]
fn thaw_more_than_available_is_clamped() {
    let mut grid = grid_with_scrollback(5, 20, 50);

    grid.compress_old_scrollback(10);
    assert_eq!(grid.compressed_history_len(), 40);

    grid.thaw_compressed_history(999);
    assert_eq!(grid.history_size(), 50);
    assert_eq!(grid.compressed_history_len(), 0);
}

#[test]
fn clear_history_clears_compressed() {
    let mut grid = grid_with_scrollback(5, 20, 50);

    grid.compress_old_scrollback(10);
    assert_eq!(grid.compressed_history_len(), 40);

    grid.clear_history();
    assert_eq!(grid.history_size(), 0);
    assert_eq!(grid.compressed_history_len(), 0);
}

#[test]
fn display_offset_clamped_after_compress() {
    let screen = 10;
    let columns = 40;
    let history = 100;

    let mut grid = grid_with_scrollback(screen, columns, history);

    // Scroll to the top of history.
    grid.scroll_display(Scroll::Top);
    assert_eq!(grid.display_offset, history);

    // Compress most of it — display_offset should be clamped to remaining hot.
    grid.compress_old_scrollback(20);
    assert_eq!(grid.display_offset, 20);
}

#[test]
fn visible_lines_unchanged_after_compress_thaw() {
    let screen = 24;
    let columns = 80;
    let history = 200;

    let mut grid = grid_with_scrollback(screen, columns, history);

    // Snapshot visible lines.
    let mut visible_before = Vec::new();
    for line in 0..screen {
        let mut cells = Vec::new();
        for col in 0..columns {
            cells.push(grid[Line(line as i32)][Column(col)].clone());
        }
        visible_before.push(cells);
    }

    grid.compress_old_scrollback(50);
    grid.thaw_compressed_history(grid.compressed_history_len());

    let mut visible_after = Vec::new();
    for line in 0..screen {
        let mut cells = Vec::new();
        for col in 0..columns {
            cells.push(grid[Line(line as i32)][Column(col)].clone());
        }
        visible_after.push(cells);
    }

    assert_eq!(visible_before, visible_after);
}

#[test]
fn compact_scrollback_if_needed_compresses_when_over_threshold() {
    let screen = 10;
    let columns = 40;
    // Build enough history to exceed 2× screen lines (threshold = 20).
    let history = 50;

    let mut grid = grid_with_scrollback(screen, columns, history);

    // Snapshot all history before compression.
    let before = snapshot_history(&grid, columns);
    assert_eq!(grid.history_size(), history);
    assert_eq!(grid.compressed_history_len(), 0);

    grid.compact_scrollback_if_needed();

    // Hot history should be clamped to the threshold (2 × screen = 20).
    let threshold = screen * 2;
    assert_eq!(grid.history_size(), threshold);
    assert_eq!(grid.compressed_history_len(), history - threshold);
    // Total history is unchanged.
    assert_eq!(grid.total_history_size(), history);

    // Thaw everything back and verify the rows round-trip.
    grid.thaw_compressed_history(grid.compressed_history_len());
    let after = snapshot_history(&grid, columns);
    assert_eq!(before, after);
}

#[test]
fn compact_scrollback_if_needed_noop_when_below_threshold() {
    let screen = 10;
    let columns = 40;
    // History equal to threshold — should not compress.
    let history = 20;

    let mut grid = grid_with_scrollback(screen, columns, history);
    grid.compact_scrollback_if_needed();

    assert_eq!(grid.history_size(), history);
    assert_eq!(grid.compressed_history_len(), 0);
}

#[test]
fn scroll_display_with_thaw_into_compressed_territory() {
    let screen = 10;
    let columns = 40;
    let history = 100;

    let mut grid = grid_with_scrollback(screen, columns, history);

    // Snapshot history before any compression.
    let full_history = snapshot_history(&grid, columns);

    // Compress most of history, keeping only 20 hot rows.
    let keep_hot = 20;
    grid.compress_old_scrollback(keep_hot);
    assert_eq!(grid.history_size(), keep_hot);
    assert_eq!(grid.compressed_history_len(), history - keep_hot);

    // Scroll to the very top — this requires thawing all compressed rows.
    grid.scroll_display_with_thaw(Scroll::Top);

    // display_offset should now cover the full history.
    assert_eq!(grid.display_offset(), grid.history_size());
    assert_eq!(grid.total_history_size(), history);
    // All compressed rows should have been thawed.
    assert_eq!(grid.compressed_history_len(), 0);

    // Verify the restored history matches the original.
    let restored = snapshot_history(&grid, columns);
    assert_eq!(full_history, restored);
}

#[test]
fn scroll_display_with_thaw_page_up_incremental() {
    let screen = 10;
    let columns = 40;
    let history = 60;

    let mut grid = grid_with_scrollback(screen, columns, history);

    // Compress, keeping 10 hot rows.
    grid.compress_old_scrollback(10);
    assert_eq!(grid.history_size(), 10);
    assert_eq!(grid.compressed_history_len(), 50);

    // PageUp once — offset goes from 0 to 10 (one page = screen lines).
    // This stays within hot history, no thaw needed.
    grid.scroll_display_with_thaw(Scroll::PageUp);
    assert_eq!(grid.display_offset(), 10);
    assert_eq!(grid.compressed_history_len(), 50);

    // PageUp again — offset wants to go to 20 but only 10 hot remain,
    // so 10 compressed rows must be thawed.
    grid.scroll_display_with_thaw(Scroll::PageUp);
    assert_eq!(grid.display_offset(), 20);
    assert!(grid.compressed_history_len() <= 40);
}

#[test]
fn scroll_display_with_thaw_bottom_does_not_thaw() {
    let screen = 10;
    let columns = 40;
    let history = 50;

    let mut grid = grid_with_scrollback(screen, columns, history);
    grid.compress_old_scrollback(10);
    let compressed_before = grid.compressed_history_len();

    // Scrolling to bottom should not thaw anything.
    grid.scroll_display_with_thaw(Scroll::Bottom);
    assert_eq!(grid.display_offset(), 0);
    assert_eq!(grid.compressed_history_len(), compressed_before);
}

