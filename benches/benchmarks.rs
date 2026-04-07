//! Performance benchmarks for Basilisk terminal emulator
//!
//! Run with: cargo bench

use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};

/// Benchmark parser throughput
fn bench_parser(c: &mut Criterion) {
    use basilisk::ansi::Parser;

    let mut group = c.benchmark_group("parser");

    // Plain ASCII text
    let ascii_text = "Hello, World! This is a test of plain ASCII text parsing.\n".repeat(1000);
    group.throughput(Throughput::Bytes(ascii_text.len() as u64));
    group.bench_function("ascii_text", |b| {
        b.iter(|| {
            let mut parser = Parser::new();
            for byte in ascii_text.bytes() {
                black_box(parser.advance(byte));
            }
        })
    });

    // CSI sequences (cursor movement, colors)
    let csi_heavy = "\x1b[1;31mRed\x1b[0m \x1b[32mGreen\x1b[0m \x1b[1;34mBlue\x1b[0m\n".repeat(1000);
    group.throughput(Throughput::Bytes(csi_heavy.len() as u64));
    group.bench_function("csi_sequences", |b| {
        b.iter(|| {
            let mut parser = Parser::new();
            for byte in csi_heavy.bytes() {
                black_box(parser.advance(byte));
            }
        })
    });

    // Mixed content (typical terminal output)
    let mixed = "$ ls -la\n\x1b[1;34mdir1\x1b[0m  \x1b[32mfile.txt\x1b[0m  README.md\n".repeat(500);
    group.throughput(Throughput::Bytes(mixed.len() as u64));
    group.bench_function("mixed_content", |b| {
        b.iter(|| {
            let mut parser = Parser::new();
            for byte in mixed.bytes() {
                black_box(parser.advance(byte));
            }
        })
    });

    group.finish();
}

/// Benchmark terminal grid operations
fn bench_grid(c: &mut Criterion) {
    use basilisk::term::grid::Grid;

    let mut group = c.benchmark_group("grid");

    group.bench_function("scroll_up_100_lines", |b| {
        let mut grid = Grid::new(80, 24, 10000);
        b.iter(|| {
            for _ in 0..100 {
                grid.scroll_up(1);
            }
        })
    });

    group.bench_function("clear_screen", |b| {
        let mut grid = Grid::new(80, 24, 10000);
        // Fill with content
        for row in 0..24 {
            for col in 0..80 {
                grid.cell_mut(col, row).c = 'X';
            }
        }
        b.iter(|| {
            grid.clear();
        })
    });

    group.bench_function("resize_larger", |b| {
        b.iter(|| {
            let mut grid = Grid::new(80, 24, 1000);
            grid.resize(120, 40);
            black_box(&grid);
        })
    });

    group.finish();
}

/// Benchmark terminal processing (parser + grid updates)
fn bench_terminal(c: &mut Criterion) {
    use basilisk::term::Terminal;

    let mut group = c.benchmark_group("terminal");

    // Simulate typical shell output
    let shell_output = "drwxr-xr-x  10 user  staff   320 Jan  1 12:00 \x1b[1;34mDocuments\x1b[0m\n\
                        -rw-r--r--   1 user  staff  1234 Jan  1 12:00 README.md\n\
                        -rwxr-xr-x   1 user  staff  5678 Jan  1 12:00 \x1b[1;32mscript.sh\x1b[0m\n"
        .repeat(100);

    group.throughput(Throughput::Bytes(shell_output.len() as u64));
    group.bench_function("process_shell_output", |b| {
        b.iter(|| {
            let mut term = Terminal::new(80, 24, 1000);
            term.process(black_box(shell_output.as_bytes()));
        })
    });

    // Heavy escape sequence processing
    let escape_heavy = (0..1000)
        .map(|i| format!("\x1b[{};{}H\x1b[38;2;{};{};{}mX", i % 24, i % 80, i % 256, (i * 2) % 256, (i * 3) % 256))
        .collect::<String>();

    group.throughput(Throughput::Bytes(escape_heavy.len() as u64));
    group.bench_function("process_escape_heavy", |b| {
        b.iter(|| {
            let mut term = Terminal::new(80, 24, 1000);
            term.process(black_box(escape_heavy.as_bytes()));
        })
    });

    group.finish();
}

/// Benchmark atlas operations
fn bench_atlas(c: &mut Criterion) {
    use basilisk::render::atlas::{Atlas, GlyphKey};

    let mut group = c.benchmark_group("atlas");

    group.bench_function("lookup_cached", |b| {
        let mut atlas = Atlas::new(2048, 2048);
        // Pre-cache some glyphs
        for c in 'A'..='Z' {
            atlas.glyphs.insert(GlyphKey::regular(c), basilisk::render::atlas::GlyphInfo {
                atlas_x: 0,
                atlas_y: 0,
                width: 10,
                height: 16,
                metrics: basilisk::render::glyph::GlyphMetrics {
                    advance_width: 10.0,
                    width: 10,
                    height: 16,
                    xmin: 0,
                    ymin: 0,
                },
            });
        }

        b.iter(|| {
            for c in 'A'..='Z' {
                black_box(atlas.get(&GlyphKey::regular(c)));
            }
        })
    });

    group.bench_function("allocate_space", |b| {
        b.iter(|| {
            let mut atlas = Atlas::new(2048, 2048);
            for _ in 0..100 {
                black_box(atlas.allocate(20, 30));
            }
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_parser,
    bench_grid,
    bench_terminal,
    bench_atlas,
);
criterion_main!(benches);
