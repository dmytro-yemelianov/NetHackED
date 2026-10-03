//! Line-oriented stdio server loop tolerant of invalid UTF-8.

use std::io::{self, BufRead, Write};

/// Reads `\n`-terminated lines, skips blank ones, and writes each handler reply on its own line.
/// Lines that are not valid UTF-8 are passed to the handler as `Err(())` instead of aborting.
pub fn serve_lines<R: BufRead, W: Write>(
    mut reader: R,
    mut writer: W,
    mut handler: impl FnMut(Result<&str, ()>) -> Option<String>,
) -> io::Result<()> {
    let mut buf = Vec::new();
    loop {
        buf.clear();
        if reader.read_until(b'\n', &mut buf)? == 0 {
            return Ok(());
        }
        let line = std::str::from_utf8(&buf).map(|s| s.trim()).map_err(|_| ());
        if matches!(line, Ok("")) {
            continue;
        }
        if let Some(out) = handler(line) {
            writeln!(writer, "{}", out)?;
            writer.flush()?;
        }
    }
}
