use crate::config::{atomic_write, Config};
use crate::error::{Result, TlError};
use crate::lock::FileLock;
use crate::parser::{self, find_last_section, find_section_end, today_str};
use crate::state::State;

/// Ensure today's section exists in the log. Returns the full content after modification.
fn ensure_today_section(content: &str) -> String {
    let today = today_str();

    if let Some((_, date)) = find_last_section(content) {
        if date == today {
            return content.to_string();
        }
    }

    let mut result = content.to_string();
    if !result.is_empty() && !result.ends_with('\n') {
        result.push('\n');
    }
    result.push('\n');
    result.push_str(&format!("### {}\n", today));
    result
}

/// Initialize the tool: create config dir, files, and today's section.
/// If `log_path` is provided, store it in config. Otherwise use default.
pub fn init(log_path: Option<&str>) -> Result<()> {
    Config::ensure_dir()?;

    let config_path = Config::config_path();
    if !config_path.exists() {
        let config = match log_path {
            Some(p) => Config::with_log_path(p),
            None => Config::default(),
        };
        config.save()?;
    } else if let Some(p) = log_path {
        // Config exists but user is updating the log path
        let mut config = Config::load()?;
        config.log_path = p.to_string();
        config.save()?;
    }

    let state_path = Config::state_path();
    if !state_path.exists() {
        State::default().save()?;
    }

    let config = Config::load()?;
    let resolved = config.resolved_log_path();

    if !resolved.exists() {
        let today = today_str();
        let content = format!("### {}\n", today);
        atomic_write(&resolved, content.as_bytes())?;
    } else {
        let content = std::fs::read_to_string(&resolved)?;
        if content.trim().is_empty() {
            let today = today_str();
            let content = format!("### {}\n", today);
            atomic_write(&resolved, content.as_bytes())?;
        } else {
            let updated = ensure_today_section(&content);
            if updated != content {
                atomic_write(&resolved, updated.as_bytes())?;
            }
        }
    }

    Ok(())
}

/// Add a new task with the given tag and title.
/// Returns the assigned task ID string.
pub fn add_task(tag: &str, title: &str) -> Result<String> {
    if !tag.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()) || tag.is_empty() {
        return Err(TlError::Parse(
            "tag must be lowercase alphanumeric".to_string(),
        ));
    }

    let _lock = FileLock::acquire()?;
    let config = Config::load()?;
    let mut state = State::load()?;

    let log_path = config.resolved_log_path();
    let content = std::fs::read_to_string(&log_path)?;
    let content = ensure_today_section(&content);

    let sections = parser::parse_log(&content, config.scan_window_lines);

    // Find the highest existing number for this tag in the log
    let max_in_log = sections
        .iter()
        .flat_map(|s| &s.tasks)
        .filter(|t| t.tag == tag)
        .map(|t| t.number)
        .max()
        .unwrap_or(0);

    // Ensure state counter is at least as high as what's in the log
    state.sync_min(tag, max_in_log);
    let number = state.next_id(tag);
    let id = format!("{}-{}", tag, number);

    let (section_line, _) = find_last_section(&content)
        .ok_or_else(|| TlError::Other("no section found in log".to_string()))?;
    let section_end = find_section_end(&content, section_line);

    let task_line = format!("- [ ] {} {}", id, title);

    let mut lines: Vec<&str> = content.lines().collect();

    if section_end >= lines.len() {
        lines.push(&task_line);
    } else {
        lines.insert(section_end, &task_line);
    }

    let mut new_content = lines.join("\n");
    if !new_content.ends_with('\n') {
        new_content.push('\n');
    }

    atomic_write(&log_path, new_content.as_bytes())?;
    state.save()?;

    Ok(id)
}

/// Mark a task as done by its ID.
pub fn complete_task(id: &str) -> Result<()> {
    let _lock = FileLock::acquire()?;
    let config = Config::load()?;

    let log_path = config.resolved_log_path();
    let content = std::fs::read_to_string(&log_path)?;

    let sections = parser::parse_log(&content, config.scan_window_lines);
    let task = parser::find_task(&sections, id)?;

    if task.done {
        return Err(TlError::Other(format!("task {} is already done", id)));
    }

    let stamp = chrono::Local::now().format("%d/%m/%Y %I:%M%p").to_string();

    let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
    let line = &mut lines[task.line_number];
    *line = format!("{} ({})", line.replacen("[ ]", "[x]", 1), stamp);

    let mut new_content = lines.join("\n");
    if !new_content.ends_with('\n') {
        new_content.push('\n');
    }

    atomic_write(&log_path, new_content.as_bytes())?;
    Ok(())
}

/// Add a note under a task by its ID.
pub fn add_note(id: &str, text: &str) -> Result<()> {
    let _lock = FileLock::acquire()?;
    let config = Config::load()?;

    let log_path = config.resolved_log_path();
    let content = std::fs::read_to_string(&log_path)?;

    let sections = parser::parse_log(&content, config.scan_window_lines);
    let task = parser::find_task(&sections, id)?;

    let insert_after = if task.notes.is_empty() {
        task.line_number
    } else {
        task.notes.last().unwrap().line_number
    };

    let indent = " ".repeat(config.note_indent);
    let stamp = chrono::Local::now().format("%d/%m/%Y %I:%M%p").to_string();
    let note_line = format!("{}- [{}] {}", indent, stamp, text);

    let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
    lines.insert(insert_after + 1, note_line);

    let mut new_content = lines.join("\n");
    if !new_content.ends_with('\n') {
        new_content.push('\n');
    }

    atomic_write(&log_path, new_content.as_bytes())?;
    Ok(())
}

/// Get today's section text.
pub fn get_today() -> Result<String> {
    let config = Config::load()?;
    let log_path = config.resolved_log_path();
    if !log_path.exists() {
        return Err(TlError::NotInitialized);
    }
    let content = std::fs::read_to_string(&log_path)?;
    parser::get_today_section_text(&content)
        .ok_or_else(|| TlError::Other("no section for today found".to_string()))
}

/// Search tasks within the scan window.
pub fn search(query: &str) -> Result<Vec<parser::Task>> {
    let config = Config::load()?;
    let log_path = config.resolved_log_path();
    if !log_path.exists() {
        return Err(TlError::NotInitialized);
    }
    let content = std::fs::read_to_string(&log_path)?;
    let sections = parser::parse_log(&content, config.scan_window_lines);
    Ok(parser::search_tasks(&sections, query))
}
