//! Integration tests for terminal functionality

use basilisk::term::{Terminal, Grid, Cell, CellFlags, Color};
use basilisk::ansi::{Parser, Action};

#[test]
fn terminal_echo_text() {
    let mut term = Terminal::new(80, 24, 1000);
    term.process(b"Hello, World!");

    // Check text appears in grid
    let grid = term.grid();
    assert_eq!(grid.cell(0, 0).c(), 'H');
    assert_eq!(grid.cell(1, 0).c(), 'e');
    assert_eq!(grid.cell(12, 0).c(), '!');
}

#[test]
fn terminal_newline() {
    let mut term = Terminal::new(80, 24, 1000);
    // Use CR+LF for proper newline behavior
    term.process(b"Line1\r\nLine2");

    let grid = term.grid();
    assert_eq!(grid.cell(0, 0).c(), 'L');
    assert_eq!(grid.cell(0, 1).c(), 'L');
}

#[test]
fn terminal_carriage_return() {
    let mut term = Terminal::new(80, 24, 1000);
    term.process(b"XXXX\rYY");

    let grid = term.grid();
    assert_eq!(grid.cell(0, 0).c(), 'Y');
    assert_eq!(grid.cell(1, 0).c(), 'Y');
    assert_eq!(grid.cell(2, 0).c(), 'X');
}

#[test]
fn terminal_cursor_movement() {
    let mut term = Terminal::new(80, 24, 1000);
    
    // Move to position 10,5 using CSI H
    term.process(b"\x1b[6;11H"); // 1-indexed: line 6, col 11
    term.process(b"X");

    let grid = term.grid();
    assert_eq!(grid.cell(10, 5).c(), 'X');
}

#[test]
fn terminal_clear_screen() {
    let mut term = Terminal::new(80, 24, 1000);
    term.process(b"XXXXX");
    term.process(b"\x1b[2J"); // Clear screen
    term.process(b"\x1b[H"); // Home

    let grid = term.grid();
    // Screen should be cleared
    assert_eq!(grid.cell(0, 0).c(), ' ');
}

#[test]
fn terminal_sgr_bold() {
    let mut term = Terminal::new(80, 24, 1000);
    term.process(b"\x1b[1mBold\x1b[0m");

    let grid = term.grid();
    assert!(grid.cell(0, 0).flags.contains(CellFlags::BOLD));
    assert!(!grid.cell(4, 0).flags.contains(CellFlags::BOLD)); // After reset
}

#[test]
fn terminal_sgr_colors() {
    let mut term = Terminal::new(80, 24, 1000);
    term.process(b"\x1b[31mR\x1b[32mG\x1b[34mB\x1b[0m");

    let grid = term.grid();
    // Red character
    assert!(grid.cell(0, 0).fg.r > grid.cell(0, 0).fg.g);
    // Green character
    assert!(grid.cell(1, 0).fg.g > grid.cell(1, 0).fg.r);
    // Blue character
    assert!(grid.cell(2, 0).fg.b > grid.cell(2, 0).fg.r);
}

#[test]
fn terminal_erase_in_line() {
    let mut term = Terminal::new(80, 24, 1000);
    term.process(b"ABCDEFGH");
    term.process(b"\x1b[4G"); // Move to column 4
    term.process(b"\x1b[K"); // Erase to end of line

    let grid = term.grid();
    assert_eq!(grid.cell(0, 0).c(), 'A');
    assert_eq!(grid.cell(2, 0).c(), 'C');
    assert_eq!(grid.cell(3, 0).c(), ' '); // Erased
    assert_eq!(grid.cell(7, 0).c(), ' '); // Erased
}

#[test]
fn terminal_scrollback() {
    let mut term = Terminal::new(80, 5, 100);
    
    // Fill screen and then some
    for i in 0..10 {
        term.process(format!("Line {}\n", i).as_bytes());
    }

    let grid = term.grid();
    // Should have scrollback
    assert!(grid.scrollback_len() > 0);
}

#[test]
fn terminal_resize() {
    let mut term = Terminal::new(80, 24, 1000);
    term.process(b"Hello");
    
    // Get cell content before resize
    let before = term.grid().cell(0, 0).c();
    assert_eq!(before, 'H');
    
    term.resize(40, 12);
    
    // Verify dimensions changed
    let grid = term.grid();
    assert_eq!(grid.cols(), 40);
    assert_eq!(grid.lines(), 12);
    // Note: resize may or may not preserve content depending on implementation
}

#[test]
fn terminal_tab() {
    let mut term = Terminal::new(80, 24, 1000);
    term.process(b"A\tB");

    let grid = term.grid();
    assert_eq!(grid.cell(0, 0).c(), 'A');
    // Tab moves to column 8
    assert_eq!(grid.cell(8, 0).c(), 'B');
}

#[test]
fn terminal_save_restore_cursor() {
    let mut term = Terminal::new(80, 24, 1000);
    term.process(b"\x1b[5;10H"); // Move to 5,10
    term.process(b"\x1b[s"); // Save cursor
    term.process(b"\x1b[1;1H"); // Move to 1,1
    term.process(b"\x1b[u"); // Restore cursor
    term.process(b"X");

    let grid = term.grid();
    assert_eq!(grid.cell(9, 4).c(), 'X'); // Back at saved position
}

#[test]
fn terminal_window_title() {
    let mut term = Terminal::new(80, 24, 1000);
    term.process(b"\x1b]0;My Title\x07");

    assert_eq!(term.title(), "My Title");
}

#[test]
fn parser_csi_sequence() {
    let mut parser = Parser::new();
    let mut actions = Vec::new();

    // Parse CSI 5 A (cursor up 5)
    for byte in b"\x1b[5A" {
        if let Some(action) = parser.advance(*byte) {
            actions.push(action);
        }
    }

    assert_eq!(actions.len(), 1);
    match &actions[0] {
        Action::CsiDispatch { params, action, .. } => {
            assert_eq!(*action, 'A');
            assert_eq!(params, &[5]);
        }
        _ => panic!("Expected CsiDispatch"),
    }
}

#[test]
fn parser_osc_sequence() {
    let mut parser = Parser::new();
    let mut actions = Vec::new();

    // Parse OSC 0;Title BEL
    for byte in b"\x1b]0;Test\x07" {
        if let Some(action) = parser.advance(*byte) {
            actions.push(action);
        }
    }

    assert_eq!(actions.len(), 1);
    match &actions[0] {
        Action::OscDispatch(params) => {
            assert_eq!(params.len(), 2);
        }
        _ => panic!("Expected OscDispatch"),
    }
}

#[test]
fn grid_cell_access() {
    let mut grid = Grid::new(80, 24, 1000);
    
    grid.cell_mut(10, 5).set_char('X');
    grid.cell_mut(10, 5).flags = CellFlags::BOLD;
    
    assert_eq!(grid.cell(10, 5).c(), 'X');
    assert!(grid.cell(10, 5).flags.contains(CellFlags::BOLD));
}

#[test]
fn grid_scroll() {
    let mut grid = Grid::new(80, 5, 100);
    
    // Write to first line
    for (i, c) in "FirstLine".chars().enumerate() {
        grid.cell_mut(i as u16, 0).set_char(c);
    }
    
    // Scroll up
    grid.scroll_up(1);
    
    // First line should be in scrollback
    assert_eq!(grid.scrollback_len(), 1);
    // New first line should be empty
    assert_eq!(grid.cell(0, 0).c(), ' ');
}
