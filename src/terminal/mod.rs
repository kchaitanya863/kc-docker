#![allow(dead_code)]

use anyhow::Result;
use std::io::{self, IsTerminal};

#[derive(Debug, Clone, Copy)]
pub struct WindowSize {
    pub rows: u16,
    pub cols: u16,
}

pub struct TerminalGuard {
    #[cfg(unix)]
    orig_termios: Option<libc::termios>,
}

impl TerminalGuard {
    /// Enter raw mode if stdin is a TTY
    pub fn enter_raw_mode() -> Result<Self> {
        let is_tty = io::stdin().is_terminal();
        if !is_tty {
            return Ok(Self {
                #[cfg(unix)]
                orig_termios: None,
            });
        }

        #[cfg(unix)]
        unsafe {
            let fd = libc::STDIN_FILENO;
            let mut orig = std::mem::zeroed();
            if libc::tcgetattr(fd, &mut orig) == 0 {
                let mut raw = orig;
                libc::cfmakeraw(&mut raw);
                libc::tcsetattr(fd, libc::TCSANOW, &raw);
                return Ok(Self {
                    orig_termios: Some(orig),
                });
            }
        }

        Ok(Self {
            #[cfg(unix)]
            orig_termios: None,
        })
    }

    /// Query the current terminal window size
    pub fn get_window_size() -> Option<WindowSize> {
        #[cfg(unix)]
        unsafe {
            let mut ws: libc::winsize = std::mem::zeroed();
            if libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut ws) == 0 {
                return Some(WindowSize {
                    rows: ws.ws_row,
                    cols: ws.ws_col,
                });
            }
        }
        None
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        #[cfg(unix)]
        if let Some(orig) = self.orig_termios {
            unsafe {
                libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &orig);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_terminal_guard_creation() {
        let guard = TerminalGuard::enter_raw_mode().unwrap();
        let _ = TerminalGuard::get_window_size();
        drop(guard);
    }
}
