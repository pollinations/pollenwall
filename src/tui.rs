use console::Term;
use crossterm::style::{StyledContent, Stylize};

use anyhow::Result;

pub const BEE: &str = "\u{1f41d}";
pub const BRUSH: &str = "\u{1f9f9}";

pub struct Tui {
    stdout: Term,
}
impl Tui {
    pub fn new() -> Self {
        Self {
            stdout: Term::stdout(),
        }
    }

    pub fn clear_lines(&self, lines: usize) -> Result<()> {
        self.stdout.move_cursor_up(lines)?;
        self.stdout.clear_line()?;

        Ok(())
    }
    pub fn hide_cursor(&self) -> Result<()> {
        self.stdout.hide_cursor()?;
        Ok(())
    }

    fn write_line(&self, line: StyledContent<&str>) -> Result<()> {
        self.stdout.write_line(&format!("{}", line))?;
        Ok(())
    }

    pub fn app_folder_not_found(&self) -> Result<()> {
        self.write_line(
            format!("{} App folder \"~/.pollenwall\" was not found. \"pollenwall\" has created it for you.", BEE)[..].yellow(),
        )?;
        Ok(())
    }
}
