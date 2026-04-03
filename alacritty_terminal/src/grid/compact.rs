//! Scrollback compression using bincode + zstd.
//!
//! Terminal scrollback rows are stored as dense arrays of 24-byte `Cell` structs,
//! even when most cells are trailing spaces with default attributes. This module
//! provides `CompactRow`, which compresses rows by:
//!
//! 1. Trimming trailing default cells (often 60-80% of the row).
//! 2. Serializing the remaining cells with bincode (compact binary serde).
//! 3. Compressing the result with zstd (entropy coding + dictionary matching).
//!
//! Decompression reverses the process: zstd decode → bincode deserialize →
//! pad with default cells to the original column width.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::grid::row::Row;
use crate::grid::GridCell;
use crate::index::Column;
use crate::term::cell::{Cell, Flags};

/// Zstd compression level. Level 3 is the default and offers a good balance
/// between speed and ratio. Terminal cell data compresses well even at low
/// levels because of the highly repetitive attribute patterns.
const ZSTD_LEVEL: i32 = 3;

/// Serializable representation of cells that have `CellExtra` data.
///
/// `Arc<CellExtra>` doesn't round-trip through serde in a way that preserves
/// sharing, so we strip extras before serialization and reattach them from a
/// sidecar on deserialization.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
struct ExtraEntry {
    column: u16,
    zerowidth: Vec<char>,
    underline_color: Option<crate::vte::ansi::Color>,
    hyperlink_id: Option<String>,
    hyperlink_uri: Option<String>,
}

/// A compressed representation of a terminal row.
///
/// Stores the serialized + compressed cell data in a contiguous byte buffer,
/// with a small sidecar for the rare `CellExtra` attributes that can't be
/// efficiently serialized inline.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct CompactRow {
    /// zstd-compressed bincode bytes of the occupied cells.
    data: Vec<u8>,

    /// Original number of columns (needed to pad with defaults on decompress).
    columns: u16,

    /// Number of cells that were serialized (content_length).
    content_cells: u16,

    /// Whether the WRAPLINE flag was set on the last column.
    wrapline: bool,

    /// Serialized CellExtra entries for cells that had them.
    /// Stored separately because Arc<CellExtra> doesn't serde-roundtrip well.
    #[cfg_attr(feature = "serde", serde(skip))]
    extras: Vec<ExtraEntry>,
}

impl PartialEq for CompactRow {
    fn eq(&self, other: &Self) -> bool {
        self.data == other.data
            && self.columns == other.columns
            && self.content_cells == other.content_cells
            && self.wrapline == other.wrapline
    }
}

impl Eq for CompactRow {}

impl CompactRow {
    /// Compress a `Row<Cell>` into a `CompactRow`.
    pub fn compress(row: &Row<Cell>) -> Self {
        let columns = row.len();
        if columns == 0 {
            return CompactRow {
                data: Vec::new(),
                columns: 0,
                content_cells: 0,
                wrapline: false,
                extras: Vec::new(),
            };
        }

        let wrapline = row[Column(columns - 1)].flags.contains(Flags::WRAPLINE);
        let content_len = content_length(row, columns);

        if content_len == 0 {
            return CompactRow {
                data: Vec::new(),
                columns: columns as u16,
                content_cells: 0,
                wrapline,
                extras: Vec::new(),
            };
        }

        // Collect the occupied cells, stripping WRAPLINE (stored separately)
        // and extracting CellExtra into the sidecar.
        let mut cells: Vec<Cell> = Vec::with_capacity(content_len);
        let mut extras: Vec<ExtraEntry> = Vec::new();

        for i in 0..content_len {
            let cell = &row[Column(i)];
            let mut cell_copy = cell.clone();
            cell_copy.flags &= !Flags::WRAPLINE;

            if cell.extra.is_some() {
                let hyperlink = cell.hyperlink();
                extras.push(ExtraEntry {
                    column: i as u16,
                    zerowidth: cell.zerowidth().unwrap_or(&[]).to_vec(),
                    underline_color: cell.underline_color(),
                    hyperlink_id: hyperlink.as_ref().map(|h| h.id().to_owned()),
                    hyperlink_uri: hyperlink.as_ref().map(|h| h.uri().to_owned()),
                });
                // Clear extra so bincode doesn't serialize the Arc.
                cell_copy.extra = None;
            }

            cells.push(cell_copy);
        }

        // bincode serialize, then zstd compress.
        let config = bincode::config::standard();
        let serialized = bincode::serde::encode_to_vec(&cells, config)
            .expect("bincode serialization of Vec<Cell> should not fail");

        let compressed = zstd::encode_all(serialized.as_slice(), ZSTD_LEVEL)
            .expect("zstd compression should not fail");

        CompactRow {
            data: compressed,
            columns: columns as u16,
            content_cells: content_len as u16,
            wrapline,
            extras,
        }
    }

    /// Decompress back into a full-width `Row<Cell>`.
    pub fn decompress(&self) -> Row<Cell> {
        let columns = self.columns as usize;
        let content_cells = self.content_cells as usize;

        let mut row = Row::<Cell>::new(columns);

        if content_cells == 0 {
            if self.wrapline && columns > 0 {
                row[Column(columns - 1)].flags.insert(Flags::WRAPLINE);
            }
            return row;
        }

        // zstd decompress, then bincode deserialize.
        let decompressed = zstd::decode_all(self.data.as_slice())
            .expect("zstd decompression should not fail for data we compressed");

        let config = bincode::config::standard();
        let (cells, _): (Vec<Cell>, _) = bincode::serde::decode_from_slice(&decompressed, config)
            .expect("bincode deserialization of Vec<Cell> should not fail");

        // Write cells into the row.
        for (i, cell) in cells.into_iter().enumerate() {
            if i < columns {
                row[Column(i)] = cell;
            }
        }

        // Reattach CellExtra from the sidecar.
        for entry in &self.extras {
            let col = entry.column as usize;
            if col < columns {
                let cell = &mut row[Column(col)];
                for &zw in &entry.zerowidth {
                    cell.push_zerowidth(zw);
                }
                if entry.underline_color.is_some() {
                    cell.set_underline_color(entry.underline_color);
                }
                if let (Some(id), Some(uri)) = (&entry.hyperlink_id, &entry.hyperlink_uri) {
                    use crate::term::cell::Hyperlink;
                    cell.set_hyperlink(Some(Hyperlink::new(
                        Some(id.clone()),
                        uri.clone(),
                    )));
                }
            }
        }

        // Restore WRAPLINE on the last column.
        if self.wrapline && columns > 0 {
            row[Column(columns - 1)].flags.insert(Flags::WRAPLINE);
        }

        row
    }

    /// Number of columns this row was compressed from.
    pub fn columns(&self) -> usize {
        self.columns as usize
    }

    /// Number of spans in the compressed representation.
    ///
    /// Not directly meaningful for the bincode+zstd format, but kept for test
    /// compatibility. Returns 1 if there is content, 0 otherwise.
    pub fn span_count(&self) -> usize {
        if self.content_cells > 0 { 1 } else { 0 }
    }

    /// Approximate heap memory used by this compact row, in bytes.
    pub fn heap_bytes(&self) -> usize {
        self.data.capacity()
            + self.extras.capacity() * std::mem::size_of::<ExtraEntry>()
            + self.extras.iter().map(|e| {
                e.zerowidth.capacity() * std::mem::size_of::<char>()
                    + e.hyperlink_id.as_ref().map_or(0, |s| s.capacity())
                    + e.hyperlink_uri.as_ref().map_or(0, |s| s.capacity())
            }).sum::<usize>()
    }

    /// Total number of encoded cells.
    pub fn encoded_cells(&self) -> usize {
        self.content_cells as usize
    }

    /// Whether the WRAPLINE flag was set on the original row.
    pub fn wrapline(&self) -> bool {
        self.wrapline
    }
}

/// Find the index past the last cell that carries meaningful content.
///
/// Scans backwards from the end of the row, skipping cells that are empty
/// (per `GridCell::is_empty`). WRAPLINE on the last column is handled
/// separately by the `wrapline` field, so we ignore it here.
fn content_length(row: &Row<Cell>, columns: usize) -> usize {
    for i in (0..columns).rev() {
        let cell = &row[Column(i)];
        let dominated_by_wrapline =
            i == columns - 1 && cell.flags == Flags::WRAPLINE && cell.c == ' ';
        if !cell.is_empty() && !dominated_by_wrapline {
            return i + 1;
        }
    }
    0
}

/// Dense row memory: `columns * size_of::<Cell>()` plus Vec overhead.
pub fn dense_row_bytes(columns: usize) -> usize {
    columns * std::mem::size_of::<Cell>() + std::mem::size_of::<Row<Cell>>()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vte::ansi::{Color, NamedColor, Rgb};

    fn default_cell() -> Cell {
        Cell::default()
    }

    fn cell_with_char(c: char) -> Cell {
        Cell { c, ..Cell::default() }
    }

    fn colored_cell(c: char, fg: Color, bg: Color) -> Cell {
        Cell { c, fg, bg, ..Cell::default() }
    }

    fn cell_with_flags(c: char, flags: Flags) -> Cell {
        Cell { c, flags, ..Cell::default() }
    }

    fn assert_rows_equal(original: &Row<Cell>, decompressed: &Row<Cell>, columns: usize) {
        for col in 0..columns {
            let orig = &original[Column(col)];
            let dec = &decompressed[Column(col)];
            assert_eq!(
                orig, dec,
                "Cell mismatch at column {col}: original={orig:?}, decompressed={dec:?}"
            );
        }
    }

    // -----------------------------------------------------------------------
    // Round-trip: compress then decompress must be lossless
    // -----------------------------------------------------------------------

    #[test]
    fn round_trip_empty_row() {
        let row = Row::<Cell>::new(160);
        let compact = CompactRow::compress(&row);
        let restored = compact.decompress();

        assert_eq!(compact.columns(), 160);
        assert_eq!(compact.encoded_cells(), 0);
        assert_rows_equal(&row, &restored, 160);
    }

    #[test]
    fn round_trip_single_char() {
        let mut row = Row::<Cell>::new(80);
        row[Column(0)] = cell_with_char('A');

        let compact = CompactRow::compress(&row);
        let restored = compact.decompress();

        assert_eq!(compact.encoded_cells(), 1);
        assert_rows_equal(&row, &restored, 80);
    }

    #[test]
    fn round_trip_short_text() {
        let text = "hello world";
        let mut row = Row::<Cell>::new(160);
        for (i, c) in text.chars().enumerate() {
            row[Column(i)] = cell_with_char(c);
        }

        let compact = CompactRow::compress(&row);
        let restored = compact.decompress();

        assert_eq!(compact.encoded_cells(), text.len());
        assert_rows_equal(&row, &restored, 160);
    }

    #[test]
    fn round_trip_coloured_text() {
        let mut row = Row::<Cell>::new(80);
        let red = Color::Named(NamedColor::Red);
        let green = Color::Named(NamedColor::Green);
        let blue = Color::Named(NamedColor::Blue);
        let default_bg = Color::Named(NamedColor::Background);

        row[Column(0)] = colored_cell('h', red, default_bg);
        row[Column(1)] = colored_cell('e', red, default_bg);
        row[Column(2)] = colored_cell('l', green, default_bg);
        row[Column(3)] = colored_cell('l', green, default_bg);
        row[Column(4)] = colored_cell('o', blue, default_bg);

        let compact = CompactRow::compress(&row);
        let restored = compact.decompress();

        assert_eq!(compact.encoded_cells(), 5);
        assert_rows_equal(&row, &restored, 80);
    }

    #[test]
    fn round_trip_bold_italic_text() {
        let mut row = Row::<Cell>::new(40);
        row[Column(0)] = cell_with_flags('B', Flags::BOLD);
        row[Column(1)] = cell_with_flags('I', Flags::ITALIC);
        row[Column(2)] = cell_with_flags('X', Flags::BOLD | Flags::ITALIC);

        let compact = CompactRow::compress(&row);
        let restored = compact.decompress();

        assert_rows_equal(&row, &restored, 40);
    }

    #[test]
    fn round_trip_wrapline() {
        let mut row = Row::<Cell>::new(5);
        for i in 0..5 {
            row[Column(i)] = cell_with_char((b'a' + i as u8) as char);
        }
        row[Column(4)].flags.insert(Flags::WRAPLINE);

        let compact = CompactRow::compress(&row);
        let restored = compact.decompress();

        assert!(compact.wrapline());
        assert_eq!(compact.encoded_cells(), 5);
        assert_rows_equal(&row, &restored, 5);
    }

    #[test]
    fn round_trip_wrapline_with_trailing_spaces() {
        let mut row = Row::<Cell>::new(10);
        row[Column(0)] = cell_with_char('x');
        row[Column(9)].flags.insert(Flags::WRAPLINE);

        let compact = CompactRow::compress(&row);
        let restored = compact.decompress();

        assert!(compact.wrapline());
        assert_rows_equal(&row, &restored, 10);
    }

    #[test]
    fn round_trip_wide_char() {
        let mut row = Row::<Cell>::new(20);
        row[Column(0)] = Cell { c: '漢', flags: Flags::WIDE_CHAR, ..Cell::default() };
        row[Column(1)] = Cell { c: ' ', flags: Flags::WIDE_CHAR_SPACER, ..Cell::default() };
        row[Column(2)] = cell_with_char('a');

        let compact = CompactRow::compress(&row);
        let restored = compact.decompress();

        assert_rows_equal(&row, &restored, 20);
    }

    #[test]
    fn round_trip_leading_wide_char_spacer() {
        let mut row = Row::<Cell>::new(10);
        row[Column(9)] =
            Cell { c: ' ', flags: Flags::LEADING_WIDE_CHAR_SPACER, ..Cell::default() };

        let compact = CompactRow::compress(&row);
        let restored = compact.decompress();

        assert_rows_equal(&row, &restored, 10);
    }

    #[test]
    fn round_trip_rgb_colours() {
        let mut row = Row::<Cell>::new(40);
        let fg = Color::Spec(Rgb { r: 255, g: 128, b: 0 });
        let bg = Color::Spec(Rgb { r: 0, g: 0, b: 64 });
        row[Column(0)] = colored_cell('R', fg, bg);
        row[Column(1)] = colored_cell('G', fg, bg);

        let indexed_fg = Color::Indexed(196);
        let indexed_bg = Color::Indexed(17);
        row[Column(2)] = colored_cell('I', indexed_fg, indexed_bg);

        let compact = CompactRow::compress(&row);
        let restored = compact.decompress();

        assert_rows_equal(&row, &restored, 40);
    }

    #[test]
    fn round_trip_zerowidth_chars() {
        let mut row = Row::<Cell>::new(20);
        let mut cell = cell_with_char('e');
        cell.push_zerowidth('\u{0301}');
        row[Column(0)] = cell;
        row[Column(1)] = cell_with_char('x');

        let compact = CompactRow::compress(&row);
        let restored = compact.decompress();

        assert_rows_equal(&row, &restored, 20);
    }

    #[test]
    fn round_trip_hyperlink() {
        use crate::term::cell::Hyperlink;

        let mut row = Row::<Cell>::new(20);
        let mut cell = cell_with_char('L');
        cell.set_hyperlink(Some(Hyperlink::new(Some("id1"), "https://example.com".to_string())));
        row[Column(0)] = cell;
        row[Column(1)] = cell_with_char('x');

        let compact = CompactRow::compress(&row);
        let restored = compact.decompress();

        assert_rows_equal(&row, &restored, 20);
    }

    #[test]
    fn round_trip_underline_colour() {
        let mut row = Row::<Cell>::new(20);
        let mut cell = cell_with_char('U');
        cell.flags.insert(Flags::UNDERLINE);
        cell.set_underline_color(Some(Color::Spec(Rgb { r: 255, g: 0, b: 0 })));
        row[Column(0)] = cell;

        let compact = CompactRow::compress(&row);
        let restored = compact.decompress();

        assert_rows_equal(&row, &restored, 20);
    }

    #[test]
    fn round_trip_non_default_background() {
        let mut row = Row::<Cell>::new(40);
        let bg = Color::Named(NamedColor::Blue);
        for i in 0..40 {
            row[Column(i)] = Cell { bg, ..Cell::default() };
        }

        let compact = CompactRow::compress(&row);
        let restored = compact.decompress();

        assert_eq!(compact.encoded_cells(), 40);
        assert_rows_equal(&row, &restored, 40);
    }

    #[test]
    fn round_trip_strikeout() {
        let mut row = Row::<Cell>::new(10);
        row[Column(0)] = cell_with_flags('S', Flags::STRIKEOUT);

        let compact = CompactRow::compress(&row);
        let restored = compact.decompress();

        assert_rows_equal(&row, &restored, 10);
    }

    #[test]
    fn round_trip_all_underline_variants() {
        let mut row = Row::<Cell>::new(20);
        row[Column(0)] = cell_with_flags('1', Flags::UNDERLINE);
        row[Column(1)] = cell_with_flags('2', Flags::DOUBLE_UNDERLINE);
        row[Column(2)] = cell_with_flags('3', Flags::UNDERCURL);
        row[Column(3)] = cell_with_flags('4', Flags::DOTTED_UNDERLINE);
        row[Column(4)] = cell_with_flags('5', Flags::DASHED_UNDERLINE);

        let compact = CompactRow::compress(&row);
        let restored = compact.decompress();

        assert_rows_equal(&row, &restored, 20);
    }

    #[test]
    fn round_trip_inverse_and_hidden() {
        let mut row = Row::<Cell>::new(10);
        row[Column(0)] = cell_with_flags('I', Flags::INVERSE);
        row[Column(1)] = cell_with_flags('H', Flags::HIDDEN);
        row[Column(2)] = cell_with_flags('D', Flags::DIM);

        let compact = CompactRow::compress(&row);
        let restored = compact.decompress();

        assert_rows_equal(&row, &restored, 10);
    }

    #[test]
    fn round_trip_tab_characters() {
        let mut row = Row::<Cell>::new(20);
        row[Column(0)] = Cell { c: '\t', ..Cell::default() };
        row[Column(1)] = cell_with_char('x');

        let compact = CompactRow::compress(&row);
        let restored = compact.decompress();

        assert_rows_equal(&row, &restored, 20);
    }

    #[test]
    fn round_trip_content_gap() {
        let mut row = Row::<Cell>::new(20);
        row[Column(0)] = cell_with_char('A');
        row[Column(19)] = cell_with_char('Z');

        let compact = CompactRow::compress(&row);
        let restored = compact.decompress();

        assert_eq!(compact.encoded_cells(), 20);
        assert_rows_equal(&row, &restored, 20);
    }

    #[test]
    fn round_trip_full_row() {
        let mut row = Row::<Cell>::new(80);
        for i in 0..80 {
            row[Column(i)] = cell_with_char((b'!' + (i % 94) as u8) as char);
        }

        let compact = CompactRow::compress(&row);
        let restored = compact.decompress();

        assert_eq!(compact.encoded_cells(), 80);
        assert_rows_equal(&row, &restored, 80);
    }

    #[test]
    fn round_trip_one_column_row() {
        let mut row = Row::<Cell>::new(1);
        row[Column(0)] = cell_with_char('X');

        let compact = CompactRow::compress(&row);
        let restored = compact.decompress();

        assert_rows_equal(&row, &restored, 1);
    }

    #[test]
    fn round_trip_alternating_colours() {
        let mut row = Row::<Cell>::new(100);
        let red = Color::Named(NamedColor::Red);
        let blue = Color::Named(NamedColor::Blue);
        let default_bg = Color::Named(NamedColor::Background);

        for i in 0..50 {
            let fg = if i % 2 == 0 { red } else { blue };
            row[Column(i)] = colored_cell((b'a' + (i % 26) as u8) as char, fg, default_bg);
        }

        let compact = CompactRow::compress(&row);
        let restored = compact.decompress();

        assert_eq!(compact.encoded_cells(), 50);
        assert_rows_equal(&row, &restored, 100);
    }

    // -----------------------------------------------------------------------
    // Compression ratio tests
    // -----------------------------------------------------------------------

    #[test]
    fn compression_ratio_empty_row() {
        let row = Row::<Cell>::new(160);
        let compact = CompactRow::compress(&row);

        let dense = dense_row_bytes(160);
        let compressed = compact.heap_bytes() + std::mem::size_of::<CompactRow>();

        let ratio = dense as f64 / compressed as f64;
        eprintln!("Empty row: dense={dense}, compressed={compressed}, ratio={ratio:.1}×");

        assert!(
            ratio > 30.0,
            "Empty row: dense={dense}, compressed={compressed}, ratio={ratio:.1}×",
        );
    }

    #[test]
    fn compression_ratio_short_line() {
        let mut row = Row::<Cell>::new(160);
        for (i, c) in "cargo build --release".chars().enumerate() {
            row[Column(i)] = cell_with_char(c);
        }

        let compact = CompactRow::compress(&row);

        let dense = dense_row_bytes(160);
        let compressed = compact.heap_bytes() + std::mem::size_of::<CompactRow>();

        let ratio = dense as f64 / compressed as f64;
        eprintln!("Short line: dense={dense}, compressed={compressed}, ratio={ratio:.1}×");

        assert!(
            ratio > 10.0,
            "Short line: dense={dense}, compressed={compressed}, ratio={ratio:.1}×",
        );
    }

    #[test]
    fn compression_ratio_coloured_ls_output() {
        let mut row = Row::<Cell>::new(160);
        let colours = [
            Color::Named(NamedColor::Blue),
            Color::Named(NamedColor::Green),
            Color::Named(NamedColor::Red),
            Color::Named(NamedColor::Cyan),
            Color::Named(NamedColor::Yellow),
        ];
        let default_bg = Color::Named(NamedColor::Background);

        let mut col = 0;
        for (file_idx, name) in
            ["Cargo.toml", "src/", "README.md", "target/", "tests/"].iter().enumerate()
        {
            let fg = colours[file_idx % colours.len()];
            for c in name.chars() {
                if col < 160 {
                    row[Column(col)] = colored_cell(c, fg, default_bg);
                    col += 1;
                }
            }
            if col < 160 {
                row[Column(col)] = default_cell();
                col += 1;
            }
            if col < 160 {
                row[Column(col)] = default_cell();
                col += 1;
            }
        }

        let compact = CompactRow::compress(&row);

        let dense = dense_row_bytes(160);
        let compressed = compact.heap_bytes() + std::mem::size_of::<CompactRow>();

        let ratio = dense as f64 / compressed as f64;
        eprintln!("ls output: dense={dense}, compressed={compressed}, ratio={ratio:.1}×");

        assert!(
            ratio > 5.0,
            "ls output: dense={dense}, compressed={compressed}, ratio={ratio:.1}×",
        );
    }

    #[test]
    fn compression_ratio_full_line_single_colour() {
        let mut row = Row::<Cell>::new(200);
        for i in 0..200 {
            row[Column(i)] = cell_with_char((b'!' + (i % 94) as u8) as char);
        }

        let compact = CompactRow::compress(&row);

        let dense = dense_row_bytes(200);
        let compressed = compact.heap_bytes() + std::mem::size_of::<CompactRow>();

        let ratio = dense as f64 / compressed as f64;
        eprintln!("Full single-colour: dense={dense}, compressed={compressed}, ratio={ratio:.1}×");

        assert!(
            ratio > 5.0,
            "Full single-colour: dense={dense}, compressed={compressed}, ratio={ratio:.1}×",
        );
    }

    // -----------------------------------------------------------------------
    // Aggregate memory savings test
    // -----------------------------------------------------------------------

    #[test]
    fn aggregate_scrollback_memory_savings() {
        let columns = 160;
        let scrollback_lines = 10_000;

        let dense_total = dense_row_bytes(columns) * scrollback_lines;

        let mut compressed_total: usize = 0;

        for line in 0..scrollback_lines {
            let mut row = Row::<Cell>::new(columns);
            match line % 10 {
                // 30% empty lines
                0 | 1 | 2 => {},
                // 40% short lines (prompts, short output)
                3 | 4 | 5 | 6 => {
                    let text_len = 20 + (line % 60);
                    for i in 0..text_len.min(columns) {
                        row[Column(i)] = cell_with_char((b'a' + (i % 26) as u8) as char);
                    }
                },
                // 20% medium coloured lines
                7 | 8 => {
                    let colours = [
                        Color::Named(NamedColor::Red),
                        Color::Named(NamedColor::Green),
                        Color::Named(NamedColor::Blue),
                    ];
                    let default_bg = Color::Named(NamedColor::Background);
                    let text_len = 60 + (line % 40);
                    for i in 0..text_len.min(columns) {
                        let fg = colours[i / 20 % colours.len()];
                        row[Column(i)] =
                            colored_cell((b'A' + (i % 26) as u8) as char, fg, default_bg);
                    }
                },
                // 10% full wrapped lines
                _ => {
                    for i in 0..columns {
                        row[Column(i)] = cell_with_char((b'!' + (i % 94) as u8) as char);
                    }
                    row[Column(columns - 1)].flags.insert(Flags::WRAPLINE);
                },
            }

            let compact = CompactRow::compress(&row);
            compressed_total += compact.heap_bytes() + std::mem::size_of::<CompactRow>();
        }

        let ratio = dense_total as f64 / compressed_total as f64;

        eprintln!(
            "Aggregate: dense={dense_total}, compressed={compressed_total}, ratio={ratio:.1}×"
        );

        assert!(
            ratio > 10.0,
            "Aggregate: dense={dense_total}, compressed={compressed_total}, ratio={ratio:.1}×"
        );
    }

    // -----------------------------------------------------------------------
    // Edge cases and invariants
    // -----------------------------------------------------------------------

    #[test]
    fn wrapline_not_in_span_flags() {
        let mut row = Row::<Cell>::new(5);
        for i in 0..5 {
            row[Column(i)] = cell_with_char((b'a' + i as u8) as char);
        }
        row[Column(4)].flags.insert(Flags::WRAPLINE);

        let compact = CompactRow::compress(&row);
        let restored = compact.decompress();

        assert!(!restored[Column(3)].flags.contains(Flags::WRAPLINE));
        assert!(restored[Column(4)].flags.contains(Flags::WRAPLINE));
        assert!(compact.wrapline());
    }

    #[test]
    fn wrapline_only_on_empty_row() {
        let mut row = Row::<Cell>::new(10);
        row[Column(9)].flags.insert(Flags::WRAPLINE);

        let compact = CompactRow::compress(&row);
        let restored = compact.decompress();

        assert!(compact.wrapline());
        assert_eq!(compact.encoded_cells(), 0);
        assert_rows_equal(&row, &restored, 10);
    }

    #[test]
    fn decompress_columns_match() {
        let row = Row::<Cell>::new(200);
        let compact = CompactRow::compress(&row);
        let restored = compact.decompress();

        assert_eq!(restored.len(), 200);
    }

    #[test]
    fn content_length_stops_at_last_non_empty() {
        let mut row = Row::<Cell>::new(100);
        row[Column(0)] = cell_with_char('A');
        row[Column(50)] = cell_with_char('B');

        let len = content_length(&row, 100);
        assert_eq!(len, 51);
    }

    #[test]
    fn content_length_all_empty() {
        let row = Row::<Cell>::new(80);
        assert_eq!(content_length(&row, 80), 0);
    }

    #[test]
    fn content_length_last_cell_non_empty() {
        let mut row = Row::<Cell>::new(10);
        row[Column(9)] = cell_with_char('Z');
        assert_eq!(content_length(&row, 10), 10);
    }

    #[test]
    fn compress_is_deterministic() {
        let mut row = Row::<Cell>::new(40);
        let fg = Color::Named(NamedColor::Green);
        let default_bg = Color::Named(NamedColor::Background);
        for i in 0..20 {
            row[Column(i)] = colored_cell((b'a' + (i % 26) as u8) as char, fg, default_bg);
        }

        let a = CompactRow::compress(&row);
        let b = CompactRow::compress(&row);
        assert_eq!(a, b);
    }

    // -----------------------------------------------------------------------
    // Stress: round-trip many diverse rows
    // -----------------------------------------------------------------------

    #[test]
    fn round_trip_many_rows() {
        let columns = 120;
        let named_colours = [
            NamedColor::Black,
            NamedColor::Red,
            NamedColor::Green,
            NamedColor::Yellow,
            NamedColor::Blue,
            NamedColor::Magenta,
            NamedColor::Cyan,
            NamedColor::White,
        ];

        for seed in 0..200 {
            let mut row = Row::<Cell>::new(columns);
            let content_len = (seed * 7 + 3) % (columns + 1);
            for i in 0..content_len {
                let colour_idx = (seed + i) % named_colours.len();
                let c = (b'!' + ((seed + i) % 94) as u8) as char;
                let fg = Color::Named(named_colours[colour_idx]);
                row[Column(i)] = colored_cell(c, fg, Color::Named(NamedColor::Background));

                if seed % 5 == 0 && i == content_len - 1 {
                    row[Column(i)].flags.insert(Flags::BOLD);
                }
            }
            if seed % 3 == 0 && content_len > 0 {
                row[Column(columns - 1)].flags.insert(Flags::WRAPLINE);
            }

            let compact = CompactRow::compress(&row);
            let restored = compact.decompress();

            assert_rows_equal(&row, &restored, columns);
        }
    }
}