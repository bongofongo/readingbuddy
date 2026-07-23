use std::io::{self, Write};

use anyhow::Result;

pub fn get_user_input(prompt: &str) -> Result<String> {
    print!("{prompt}");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().to_string())
}

pub fn confirm(prompt: &str) -> Result<bool> {
    Ok(matches!(
        get_user_input(&format!("{prompt} [y/N]: "))?.to_lowercase().as_str(),
        "y" | "yes"
    ))
}

/// Prompt for an index below `len`; empty input returns None.
pub fn select_element(prompt: &str, len: usize) -> Result<Option<usize>> {
    loop {
        let raw = get_user_input(prompt)?;
        if raw.is_empty() {
            return Ok(None);
        }
        match raw.parse::<usize>() {
            Ok(i) if i < len => return Ok(Some(i)),
            Ok(_) => println!("out of bounds, try again."),
            Err(e) => println!("{e}, try again."),
        }
    }
}

/// Compose text in $EDITOR (fallback vi) via a temp file.
pub fn edit_in_editor(initial: &str) -> Result<String> {
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
    let path = std::env::temp_dir().join(format!("readingbuddy-note-{}.md", std::process::id()));
    std::fs::write(&path, initial)?;
    let status = std::process::Command::new(&editor).arg(&path).status()?;
    if !status.success() {
        anyhow::bail!("{editor} exited with {status}");
    }
    let text = std::fs::read_to_string(&path)?;
    std::fs::remove_file(&path).ok();
    Ok(text.trim().to_string())
}
