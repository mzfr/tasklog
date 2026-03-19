mod config;
mod error;
mod lock;
mod mcp;
mod parser;
mod state;
mod tui;
mod writer;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "tl", about = "Minimal global markdown task log")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize tl: create config, log, and state files
    Init {
        /// Path to your existing log.md file (default: ~/.config/tl/log.md)
        #[arg(long)]
        log: Option<String>,
    },

    /// Add a new task: tl add <tag> <title>
    Add {
        /// Task tag (lowercase alphanumeric, e.g. "osv", "infra")
        tag: String,
        /// Task title
        title: Vec<String>,
        /// Mark as high priority
        #[arg(short, long)]
        priority: bool,
    },

    /// Mark a task as done: tl done <id>
    Done {
        /// Task ID (e.g. "osv-12")
        id: String,
    },

    /// Undo a completed task: tl undo <id>
    Undo {
        /// Task ID (e.g. "osv-12")
        id: String,
    },

    /// Add a note to a task: tl note <id> <text>
    Note {
        /// Task ID (e.g. "osv-12")
        id: String,
        /// Note text
        text: Vec<String>,
    },

    /// Search tasks: tl search <query>
    Search {
        /// Search query
        query: Vec<String>,
    },

    /// Show today's section
    Today,

    /// Rename a tag: tl rename <old> <new>
    Rename {
        /// Current tag name
        old: String,
        /// New tag name
        new: String,
    },

    /// Toggle priority on a task: tl priority <id>
    Priority {
        /// Task ID (e.g. "osv-12")
        id: String,
    },

    /// Edit a task's title: tl edit <id> <new title>
    Edit {
        /// Task ID (e.g. "osv-12")
        id: String,
        /// New title
        title: Vec<String>,
    },

    /// Delete a task and its notes: tl delete <id>
    Delete {
        /// Task ID (e.g. "osv-12")
        id: String,
    },

    /// Open interactive TUI
    Tui,

    /// Start MCP server (stdio transport)
    Mcp,
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Init { log } => cmd_init(log.as_deref()),
        Commands::Add {
            tag,
            title,
            priority,
        } => cmd_add(&tag, &title.join(" "), priority),
        Commands::Done { id } => cmd_done(&id),
        Commands::Undo { id } => cmd_undo(&id),
        Commands::Note { id, text } => cmd_note(&id, &text.join(" ")),
        Commands::Search { query } => cmd_search(&query.join(" ")),
        Commands::Today => cmd_today(),
        Commands::Rename { old, new } => cmd_rename(&old, &new),
        Commands::Priority { id } => cmd_priority(&id),
        Commands::Edit { id, title } => cmd_edit(&id, &title.join(" ")),
        Commands::Delete { id } => cmd_delete(&id),
        Commands::Tui => cmd_tui(),
        Commands::Mcp => cmd_mcp(),
    };

    if let Err(e) = result {
        eprintln!("error: {}", e);
        std::process::exit(1);
    }
}

fn cmd_init(log_path: Option<&str>) -> error::Result<()> {
    writer::init(log_path)?;
    let config = config::Config::load()?;
    println!("initialized at {}", config::Config::base_dir().display());
    println!("log file: {}", config.resolved_log_path().display());
    Ok(())
}

fn cmd_add(tag: &str, title: &str, priority: bool) -> error::Result<()> {
    if title.is_empty() {
        return Err(error::TlError::Other("title cannot be empty".to_string()));
    }
    let id = writer::add_task_with_priority(tag, title, priority)?;
    println!("created {}", id);
    Ok(())
}

fn cmd_done(id: &str) -> error::Result<()> {
    writer::complete_task(id)?;
    println!("completed {}", id);
    Ok(())
}

fn cmd_undo(id: &str) -> error::Result<()> {
    writer::undo_task(id)?;
    println!("reopened {}", id);
    Ok(())
}

fn cmd_note(id: &str, text: &str) -> error::Result<()> {
    if text.is_empty() {
        return Err(error::TlError::Other("note text cannot be empty".to_string()));
    }
    writer::add_note(id, text)?;
    println!("noted on {}", id);
    Ok(())
}

fn cmd_search(query: &str) -> error::Result<()> {
    if query.is_empty() {
        return Err(error::TlError::Other("search query cannot be empty".to_string()));
    }
    let tasks = writer::search(query)?;
    if tasks.is_empty() {
        println!("no tasks found matching \"{}\"", query);
        return Ok(());
    }
    for task in &tasks {
        let status = if task.done { "x" } else { " " };
        let priority = if task.priority { "!" } else { "" };
        println!("[{}] {}{} {}", status, task.id(), priority, task.title);
        for note in &task.notes {
            println!("      - {}", note.text);
        }
    }
    Ok(())
}

fn cmd_today() -> error::Result<()> {
    let text = writer::get_today()?;
    println!("{}", text);
    Ok(())
}

fn cmd_rename(old: &str, new: &str) -> error::Result<()> {
    writer::rename_tag(old, new)?;
    println!("renamed {} -> {}", old, new);
    Ok(())
}

fn cmd_priority(id: &str) -> error::Result<()> {
    let new_priority = writer::toggle_priority(id)?;
    if new_priority {
        println!("{} marked as high priority", id);
    } else {
        println!("{} marked as normal priority", id);
    }
    Ok(())
}

fn cmd_edit(id: &str, new_title: &str) -> error::Result<()> {
    if new_title.is_empty() {
        return Err(error::TlError::Other("title cannot be empty".to_string()));
    }
    writer::edit_task(id, new_title)?;
    println!("edited {}", id);
    Ok(())
}

fn cmd_delete(id: &str) -> error::Result<()> {
    writer::delete_task(id)?;
    println!("deleted {}", id);
    Ok(())
}

fn cmd_tui() -> error::Result<()> {
    tui::run()
}

fn cmd_mcp() -> error::Result<()> {
    mcp::run_mcp_server()
}
