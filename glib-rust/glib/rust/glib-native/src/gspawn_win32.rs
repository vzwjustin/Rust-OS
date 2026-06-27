//! Win32 process spawning compatibility (`gspawn-win32.c`).

use crate::gspawn_win32_helper::join_command_line;
use alloc::string::String;

pub type SpawnWin32Result<T> = Result<T, SpawnWin32Error>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpawnWin32Error {
    EmptyCommand,
    Unsupported,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpawnWin32Command {
    command_line: String,
}

impl SpawnWin32Command {
    #[must_use]
    pub fn command_line(&self) -> &str {
        &self.command_line
    }
}

pub fn prepare_command(args: &[&str]) -> SpawnWin32Result<SpawnWin32Command> {
    if args.is_empty() {
        return Err(SpawnWin32Error::EmptyCommand);
    }
    Ok(SpawnWin32Command {
        command_line: join_command_line(args),
    })
}

pub fn spawn_async(_args: &[&str]) -> SpawnWin32Result<()> {
    Err(SpawnWin32Error::Unsupported)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepares_command_line_but_does_not_spawn() {
        let command = prepare_command(&["prog", "two words"]).unwrap();
        assert_eq!(command.command_line(), "prog \"two words\"");
        assert_eq!(spawn_async(&["prog"]), Err(SpawnWin32Error::Unsupported));
    }
}
