use crate::config::Config;
use crate::error::{Result, TlError};
use crate::parser::{self, Task};
use crate::writer;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::prelude::*;
use ratatui::widgets::*;

use std::io::stdout;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Mode {
    Normal,
    AddTag,
    AddTitle,
    NoteInput,
    Search,
}

struct App {
    tasks: Vec<Task>,
    selected: usize,
    today_text: String,
    mode: Mode,
    input: String,
    add_tag: String,
    status_msg: String,
    search_query: String,
    should_quit: bool,
}

impl App {
    fn new() -> Result<Self> {
        let mut app = App {
            tasks: Vec::new(),
            selected: 0,
            today_text: String::new(),
            mode: Mode::Normal,
            input: String::new(),
            add_tag: String::new(),
            status_msg: String::from("Press ? for help"),
            search_query: String::new(),
            should_quit: false,
        };
        app.refresh()?;
        Ok(app)
    }

    fn refresh(&mut self) -> Result<()> {
        let config = Config::load()?;
        let log_path = config.resolved_log_path();
        let content = std::fs::read_to_string(&log_path)?;

        self.today_text = parser::get_today_section_text(&content)
            .unwrap_or_else(|| "No section for today.".to_string());

        let sections = parser::parse_log(&content, config.scan_window_lines);

        if self.search_query.is_empty() {
            // Show all tasks from recent sections
            self.tasks = sections
                .iter()
                .flat_map(|s| s.tasks.clone())
                .rev()
                .collect();
        } else {
            self.tasks = parser::search_tasks(&sections, &self.search_query);
            self.tasks.reverse();
        }

        if self.selected >= self.tasks.len() && !self.tasks.is_empty() {
            self.selected = self.tasks.len() - 1;
        }

        Ok(())
    }

    fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
        match self.mode {
            Mode::Normal => self.handle_normal_key(key),
            Mode::AddTag => self.handle_add_tag_key(key),
            Mode::AddTitle => self.handle_add_title_key(key),
            Mode::NoteInput => self.handle_note_input_key(key),
            Mode::Search => self.handle_search_key(key),
        }
    }

    fn handle_normal_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.should_quit = true
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if !self.tasks.is_empty() {
                    self.selected = (self.selected + 1).min(self.tasks.len() - 1);
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.selected > 0 {
                    self.selected -= 1;
                }
            }
            KeyCode::Char('g') => self.selected = 0,
            KeyCode::Char('G') => {
                if !self.tasks.is_empty() {
                    self.selected = self.tasks.len() - 1;
                }
            }
            KeyCode::Char('a') => {
                self.mode = Mode::AddTag;
                self.input.clear();
                self.add_tag.clear();
                self.status_msg = "Enter tag (then press Enter for title):".to_string();
            }
            KeyCode::Char('d') => {
                if let Some(task) = self.tasks.get(self.selected) {
                    let id = task.id();
                    match writer::complete_task(&id) {
                        Ok(()) => {
                            self.status_msg = format!("Completed {}", id);
                            self.refresh()?;
                        }
                        Err(e) => self.status_msg = format!("Error: {}", e),
                    }
                }
            }
            KeyCode::Char('n') => {
                if self.tasks.get(self.selected).is_some() {
                    self.mode = Mode::NoteInput;
                    self.input.clear();
                    self.status_msg = "Enter note text:".to_string();
                }
            }
            KeyCode::Char('/') => {
                self.mode = Mode::Search;
                self.input.clear();
                self.status_msg = "Search:".to_string();
            }
            KeyCode::Char('c') => {
                self.search_query.clear();
                self.status_msg = "Filter cleared".to_string();
                self.refresh()?;
            }
            KeyCode::Char('r') => {
                self.refresh()?;
                self.status_msg = "Refreshed".to_string();
            }
            KeyCode::Char('?') => {
                self.status_msg =
                    "j/k:nav a:add d:done n:note /:search c:clear r:refresh q:quit".to_string();
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_add_tag_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.status_msg = "Cancelled".to_string();
            }
            KeyCode::Enter => {
                if self.input.is_empty() {
                    self.status_msg = "Tag cannot be empty".to_string();
                } else {
                    self.add_tag = self.input.clone();
                    self.input.clear();
                    self.mode = Mode::AddTitle;
                    self.status_msg = format!("Tag: {} | Enter title:", self.add_tag);
                }
            }
            KeyCode::Backspace => {
                self.input.pop();
            }
            KeyCode::Char(c) if c.is_ascii_lowercase() || c.is_ascii_digit() => {
                self.input.push(c);
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_add_title_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.status_msg = "Cancelled".to_string();
            }
            KeyCode::Enter => {
                if self.input.is_empty() {
                    self.status_msg = "Title cannot be empty".to_string();
                } else {
                    match writer::add_task(&self.add_tag, &self.input) {
                        Ok(id) => {
                            self.status_msg = format!("Created {}", id);
                            self.mode = Mode::Normal;
                            self.refresh()?;
                        }
                        Err(e) => self.status_msg = format!("Error: {}", e),
                    }
                }
            }
            KeyCode::Backspace => {
                self.input.pop();
            }
            KeyCode::Char(c) => {
                self.input.push(c);
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_note_input_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.status_msg = "Cancelled".to_string();
            }
            KeyCode::Enter => {
                if self.input.is_empty() {
                    self.status_msg = "Note cannot be empty".to_string();
                } else if let Some(task) = self.tasks.get(self.selected) {
                    let id = task.id();
                    match writer::add_note(&id, &self.input) {
                        Ok(()) => {
                            self.status_msg = format!("Note added to {}", id);
                            self.mode = Mode::Normal;
                            self.refresh()?;
                        }
                        Err(e) => self.status_msg = format!("Error: {}", e),
                    }
                }
            }
            KeyCode::Backspace => {
                self.input.pop();
            }
            KeyCode::Char(c) => {
                self.input.push(c);
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_search_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.search_query.clear();
                self.status_msg = "Search cancelled".to_string();
                self.refresh()?;
            }
            KeyCode::Enter => {
                self.search_query = self.input.clone();
                self.mode = Mode::Normal;
                self.selected = 0;
                self.refresh()?;
                self.status_msg = if self.tasks.is_empty() {
                    format!("No results for \"{}\"", self.search_query)
                } else {
                    format!("{} results for \"{}\"", self.tasks.len(), self.search_query)
                };
            }
            KeyCode::Backspace => {
                self.input.pop();
            }
            KeyCode::Char(c) => {
                self.input.push(c);
            }
            _ => {}
        }
        Ok(())
    }
}

fn ui(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // header
            Constraint::Min(5),    // main content
            Constraint::Length(3), // input / status
        ])
        .split(frame.area());

    // Header
    let title = if app.search_query.is_empty() {
        " tl — task log ".to_string()
    } else {
        format!(" tl — search: \"{}\" ", app.search_query)
    };
    let header = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(Color::Cyan));
    let header_text = Paragraph::new(format!(
        " {} tasks | {} for help",
        app.tasks.len(),
        "?"
    ))
    .block(header);
    frame.render_widget(header_text, chunks[0]);

    // Main area: split into today's raw text and task list
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(chunks[1]);

    // Today's section (left panel)
    let today_block = Block::default()
        .borders(Borders::ALL)
        .title(" Today ")
        .border_style(Style::default().fg(Color::Yellow));
    let today_text = Paragraph::new(app.today_text.clone())
        .block(today_block)
        .wrap(Wrap { trim: false });
    frame.render_widget(today_text, main_chunks[0]);

    // Task list (right panel)
    let items: Vec<ListItem> = app
        .tasks
        .iter()
        .enumerate()
        .map(|(i, task)| {
            let checkbox = if task.done { "[x]" } else { "[ ]" };
            let main_line = format!("{} {} {}", checkbox, task.id(), task.title);

            let mut lines = vec![Line::from(Span::raw(main_line))];
            for note in &task.notes {
                lines.push(Line::from(Span::styled(
                    format!("    - {}", note.text),
                    Style::default().fg(Color::DarkGray),
                )));
            }

            let style = if i == app.selected {
                Style::default().bg(Color::DarkGray).fg(Color::White)
            } else if task.done {
                Style::default().fg(Color::Green)
            } else {
                Style::default()
            };

            ListItem::new(Text::from(lines)).style(style)
        })
        .collect();

    let task_list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Tasks ")
                .border_style(Style::default().fg(Color::Magenta)),
        )
        .highlight_style(Style::default().bg(Color::DarkGray));
    frame.render_widget(task_list, main_chunks[1]);

    // Status / input bar
    let input_text = match app.mode {
        Mode::Normal => app.status_msg.clone(),
        Mode::AddTag => format!("Tag: {}_", app.input),
        Mode::AddTitle => format!("[{}] Title: {}_", app.add_tag, app.input),
        Mode::NoteInput => format!("Note: {}_", app.input),
        Mode::Search => format!("/{}_", app.input),
    };
    let mode_label = match app.mode {
        Mode::Normal => "NORMAL",
        Mode::AddTag | Mode::AddTitle => "ADD",
        Mode::NoteInput => "NOTE",
        Mode::Search => "SEARCH",
    };
    let status_block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {} ", mode_label))
        .border_style(Style::default().fg(if app.mode == Mode::Normal {
            Color::Gray
        } else {
            Color::Green
        }));
    let status = Paragraph::new(input_text).block(status_block);
    frame.render_widget(status, chunks[2]);
}

pub fn run() -> Result<()> {
    // Ensure initialized
    if !Config::config_path().exists() {
        return Err(TlError::NotInitialized);
    }

    enable_raw_mode().map_err(|e| TlError::Other(e.to_string()))?;
    stdout()
        .execute(EnterAlternateScreen)
        .map_err(|e| TlError::Other(e.to_string()))?;

    let backend = CrosstermBackend::new(stdout());
    let mut terminal =
        Terminal::new(backend).map_err(|e| TlError::Other(e.to_string()))?;

    let mut app = App::new()?;

    loop {
        terminal
            .draw(|f| ui(f, &app))
            .map_err(|e| TlError::Other(e.to_string()))?;

        if event::poll(Duration::from_millis(100))
            .map_err(|e| TlError::Other(e.to_string()))?
        {
            if let Event::Key(key) = event::read().map_err(|e| TlError::Other(e.to_string()))? {
                app.handle_key(key)?;
                if app.should_quit {
                    break;
                }
            }
        }
    }

    disable_raw_mode().map_err(|e| TlError::Other(e.to_string()))?;
    stdout()
        .execute(LeaveAlternateScreen)
        .map_err(|e| TlError::Other(e.to_string()))?;

    Ok(())
}
