use std::io::{self, Write};

pub fn confirm_action(message: &str) -> bool {
    print!("{message} [y/N] ");
    io::stdout().flush().ok();
    let mut input = String::new();
    io::stdin().read_line(&mut input).ok();
    matches!(input.trim().to_lowercase().as_str(), "y" | "yes")
}
