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
    },

    /// Mark a task as done: tl done <id>
    Done {
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

    /// Open interactive TUI
    Tui,

    /// Start MCP server (stdio transport)
    Mcp,
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Init { log } => cmd_init(log.as_deref()),
        Commands::Add { tag, title } => cmd_add(&tag, &title.join(" ")),
        Commands::Done { id } => cmd_done(&id),
        Commands::Note { id, text } => cmd_note(&id, &text.join(" ")),
        Commands::Search { query } => cmd_search(&query.join(" ")),
        Commands::Today => cmd_today(),
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

fn cmd_add(tag: &str, title: &str) -> error::Result<()> {
    if title.is_empty() {
        return Err(error::TlError::Other("title cannot be empty".to_string()));
    }
    let id = writer::add_task(tag, title)?;
    println!("created {}", id);
    Ok(())
}

fn cmd_done(id: &str) -> error::Result<()> {
    writer::complete_task(id)?;
    println!("completed {}", id);
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
        println!("[{}] {} {}", status, task.id(), task.title);
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

fn cmd_tui() -> error::Result<()> {
    tui::run()
}

fn cmd_mcp() -> error::Result<()> {
    mcp::run_mcp_server()
}
