use crate::config::Config;
use crate::error::{Result, TlError};
use crate::parser::{self, Task};
use crate::writer;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::prelude::*;
use ratatui::widgets::*;

use std::collections::BTreeSet;
use std::io::stdout;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Focus {
    Projects,
    Tasks,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Mode {
    Normal,
    AddTag,
    AddTitle,
    NoteInput,
    Search,
}

struct App {
    all_tasks: Vec<Task>,
    projects: Vec<String>,
    project_idx: usize,
    task_idx: usize,
    focus: Focus,
    mode: Mode,
    input: String,
    add_tag: String,
    status_msg: String,
    search_query: String,
    show_detail: bool,
    should_quit: bool,
}

impl App {
    fn new() -> Result<Self> {
        let mut app = App {
            all_tasks: Vec::new(),
            projects: Vec::new(),
            project_idx: 0,
            task_idx: 0,
            focus: Focus::Projects,
            mode: Mode::Normal,
            input: String::new(),
            add_tag: String::new(),
            status_msg: String::from("? for help | Tab to switch panels"),
            search_query: String::new(),
            show_detail: false,
            should_quit: false,
        };
        app.refresh()?;
        Ok(app)
    }

    fn refresh(&mut self) -> Result<()> {
        let config = Config::load()?;
        let log_path = config.resolved_log_path();
        let content = std::fs::read_to_string(&log_path)?;

        let sections = parser::parse_log(&content, config.scan_window_lines);

        if self.search_query.is_empty() {
            self.all_tasks = sections
                .iter()
                .flat_map(|s| s.tasks.clone())
                .collect();
        } else {
            self.all_tasks = parser::search_tasks(&sections, &self.search_query);
        }

        // Collect unique tags sorted
        let tags: BTreeSet<String> = self.all_tasks.iter().map(|t| t.tag.clone()).collect();
        self.projects = tags.into_iter().collect();

        if self.project_idx >= self.projects.len() && !self.projects.is_empty() {
            self.project_idx = self.projects.len() - 1;
        }

        self.clamp_task_idx();
        Ok(())
    }

    fn filtered_tasks(&self) -> Vec<&Task> {
        if self.projects.is_empty() {
            return Vec::new();
        }
        let tag = &self.projects[self.project_idx];
        self.all_tasks.iter().filter(|t| t.tag == *tag).collect()
    }

    fn clamp_task_idx(&mut self) {
        let count = self.filtered_tasks().len();
        if count == 0 {
            self.task_idx = 0;
        } else if self.task_idx >= count {
            self.task_idx = count - 1;
        }
    }

    fn selected_task(&self) -> Option<&Task> {
        self.filtered_tasks().get(self.task_idx).copied()
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
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Esc => {
                if self.show_detail {
                    self.show_detail = false;
                } else {
                    self.should_quit = true;
                }
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.should_quit = true
            }
            KeyCode::Enter => {
                if self.focus == Focus::Tasks && self.selected_task().is_some() {
                    self.show_detail = !self.show_detail;
                }
            }
            KeyCode::Tab | KeyCode::BackTab => {
                self.focus = match self.focus {
                    Focus::Projects => Focus::Tasks,
                    Focus::Tasks => Focus::Projects,
                };
            }
            KeyCode::Char('h') | KeyCode::Left => self.focus = Focus::Projects,
            KeyCode::Char('l') | KeyCode::Right => self.focus = Focus::Tasks,
            KeyCode::Char('j') | KeyCode::Down => match self.focus {
                Focus::Projects => {
                    if !self.projects.is_empty() {
                        self.project_idx =
                            (self.project_idx + 1).min(self.projects.len() - 1);
                        self.task_idx = 0;
                    }
                }
                Focus::Tasks => {
                    let count = self.filtered_tasks().len();
                    if count > 0 {
                        self.task_idx = (self.task_idx + 1).min(count - 1);
                    }
                }
            },
            KeyCode::Char('k') | KeyCode::Up => match self.focus {
                Focus::Projects => {
                    if self.project_idx > 0 {
                        self.project_idx -= 1;
                        self.task_idx = 0;
                    }
                }
                Focus::Tasks => {
                    if self.task_idx > 0 {
                        self.task_idx -= 1;
                    }
                }
            },
            KeyCode::Char('g') => match self.focus {
                Focus::Projects => {
                    self.project_idx = 0;
                    self.task_idx = 0;
                }
                Focus::Tasks => self.task_idx = 0,
            },
            KeyCode::Char('G') => match self.focus {
                Focus::Projects => {
                    if !self.projects.is_empty() {
                        self.project_idx = self.projects.len() - 1;
                        self.task_idx = 0;
                    }
                }
                Focus::Tasks => {
                    let count = self.filtered_tasks().len();
                    if count > 0 {
                        self.task_idx = count - 1;
                    }
                }
            },
            KeyCode::Char('a') => {
                self.mode = Mode::AddTag;
                self.input.clear();
                self.add_tag.clear();
                self.status_msg = "Enter tag (then Enter for title):".to_string();
            }
            KeyCode::Char('d') => {
                if let Some(task) = self.selected_task() {
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
                if self.selected_task().is_some() {
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
                    "j/k:nav h/l:panel Tab:switch Enter:detail a:add d:done n:note /:search c:clear q:quit"
                        .to_string();
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
                    self.mode = Mode::Normal;
                    self.status_msg = "Title cannot be empty".to_string();
                } else {
                    match writer::add_task(&self.add_tag, &self.input) {
                        Ok(id) => {
                            self.status_msg = format!("Created {}", id);
                            self.mode = Mode::Normal;
                            self.refresh()?;
                        }
                        Err(e) => {
                            self.mode = Mode::Normal;
                            self.status_msg = format!("Error: {}", e);
                        }
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
                    self.mode = Mode::Normal;
                    self.status_msg = "Note cannot be empty".to_string();
                } else if let Some(task) = self.selected_task() {
                    let id = task.id();
                    match writer::add_note(&id, &self.input) {
                        Ok(()) => {
                            self.status_msg = format!("Note added to {}", id);
                            self.mode = Mode::Normal;
                            self.refresh()?;
                        }
                        Err(e) => {
                            self.mode = Mode::Normal;
                            self.status_msg = format!("Error: {}", e);
                        }
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
                self.project_idx = 0;
                self.task_idx = 0;
                self.refresh()?;
                self.status_msg = if self.all_tasks.is_empty() {
                    format!("No results for \"{}\"", self.search_query)
                } else {
                    format!(
                        "{} results for \"{}\"",
                        self.all_tasks.len(),
                        self.search_query
                    )
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
            Constraint::Length(3), // header
            Constraint::Min(5),   // main
            Constraint::Length(3), // status
        ])
        .split(frame.area());

    // Header
    let title = if app.search_query.is_empty() {
        " tasklog ".to_string()
    } else {
        format!(" tasklog — search: \"{}\" ", app.search_query)
    };
    let header = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(Color::Cyan));
    let header_text = Paragraph::new(format!(
        " {} projects | {} tasks | ? for help",
        app.projects.len(),
        app.all_tasks.len(),
    ))
    .block(header);
    frame.render_widget(header_text, chunks[0]);

    // Main: Projects (left) | Tasks (right)
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(20), Constraint::Min(30)])
        .split(chunks[1]);

    // Projects panel
    let project_border_color = if app.focus == Focus::Projects {
        Color::Yellow
    } else {
        Color::DarkGray
    };
    let project_items: Vec<ListItem> = app
        .projects
        .iter()
        .enumerate()
        .map(|(i, tag)| {
            let task_count = app.all_tasks.iter().filter(|t| t.tag == *tag).count();
            let open_count = app
                .all_tasks
                .iter()
                .filter(|t| t.tag == *tag && !t.done)
                .count();
            let label = format!("{} ({}/{})", tag, open_count, task_count);
            let style = if i == app.project_idx {
                Style::default().bg(Color::DarkGray).fg(Color::White)
            } else {
                Style::default()
            };
            ListItem::new(label).style(style)
        })
        .collect();

    let project_list = List::new(project_items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Projects ")
            .border_style(Style::default().fg(project_border_color)),
    );
    frame.render_widget(project_list, main_chunks[0]);

    // Tasks panel
    let task_border_color = if app.focus == Focus::Tasks {
        Color::Magenta
    } else {
        Color::DarkGray
    };
    let filtered = app.filtered_tasks();
    let task_items: Vec<ListItem> = filtered
        .iter()
        .enumerate()
        .map(|(i, task)| {
            let checkbox = if task.done { "[x]" } else { "[ ]" };
            let note_hint = if task.notes.is_empty() {
                String::new()
            } else {
                format!(" [{}]", task.notes.len())
            };
            let label = format!("{} {} {}{}", checkbox, task.id(), task.title, note_hint);

            let style = if i == app.task_idx {
                Style::default().bg(Color::DarkGray).fg(Color::White)
            } else if task.done {
                Style::default().fg(Color::Green)
            } else {
                Style::default()
            };
            ListItem::new(label).style(style)
        })
        .collect();

    let task_title = if app.projects.is_empty() {
        " Tasks ".to_string()
    } else {
        format!(" Tasks — {} ", app.projects[app.project_idx])
    };
    let task_list = List::new(task_items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(task_title)
            .border_style(Style::default().fg(task_border_color)),
    );
    frame.render_widget(task_list, main_chunks[1]);

    // Detail popup
    if app.show_detail {
        if let Some(task) = app.selected_task() {
            let area = frame.area();
            let popup_width = (area.width * 60 / 100).max(40).min(area.width.saturating_sub(4));
            let popup_height = (5 + task.notes.len() as u16 + 2).min(area.height.saturating_sub(4));
            let x = (area.width.saturating_sub(popup_width)) / 2;
            let y = (area.height.saturating_sub(popup_height)) / 2;
            let popup_area = Rect::new(x, y, popup_width, popup_height);

            // Clear the area behind the popup
            frame.render_widget(Clear, popup_area);

            let status_str = if task.done { "done" } else { "open" };
            let status_color = if task.done { Color::Green } else { Color::Yellow };

            let mut lines: Vec<Line> = Vec::new();
            lines.push(Line::from(vec![
                Span::styled("ID: ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Span::raw(task.id()),
                Span::raw("  "),
                Span::styled(
                    format!("[{}]", status_str),
                    Style::default().fg(status_color).add_modifier(Modifier::BOLD),
                ),
            ]));
            lines.push(Line::from(vec![
                Span::styled("Title: ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Span::raw(&task.title),
            ]));
            if !task.date.is_empty() {
                lines.push(Line::from(vec![
                    Span::styled("Date: ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                    Span::raw(&task.date),
                ]));
            }

            if !task.notes.is_empty() {
                lines.push(Line::from(""));
                for note in &task.notes {
                    lines.push(Line::from(vec![
                        Span::styled("  - ", Style::default().fg(Color::DarkGray)),
                        Span::styled(&note.text, Style::default().fg(Color::White)),
                    ]));
                }
            }

            let popup = Paragraph::new(Text::from(lines)).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Task Detail ")
                    .title_bottom(" Esc to close ")
                    .border_style(Style::default().fg(Color::Cyan)),
            );
            frame.render_widget(popup, popup_area);
        }
    }

    // Status bar
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
    if !Config::config_path().exists() {
        return Err(TlError::NotInitialized);
    }

    enable_raw_mode().map_err(|e| TlError::Other(e.to_string()))?;
    stdout()
        .execute(EnterAlternateScreen)
        .map_err(|e| TlError::Other(e.to_string()))?;

    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend).map_err(|e| TlError::Other(e.to_string()))?;

    let mut app = App::new()?;

    loop {
        terminal
            .draw(|f| ui(f, &app))
            .map_err(|e| TlError::Other(e.to_string()))?;

        if event::poll(Duration::from_millis(100))
            .map_err(|e| TlError::Other(e.to_string()))?
        {
            if let Event::Key(key) = event::read().map_err(|e| TlError::Other(e.to_string()))? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
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
