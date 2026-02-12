# tl

A minimal global markdown task log with a CLI, interactive TUI, and MCP server.

Inspired by [Backlog.md](https://github.com/MrLesk/Backlog.md), but built around the idea of keeping a single global `log.md` file that you already maintain by hand. `tl` overlays structured tasks on top of your existing freeform markdown without ever rewriting or restructuring your content.

Backlog.md felt more suited for team collaboration and required a separate directory setup. I already had a `log.md` file that I used as a mix of TODOs, thought dumps, and activity updates. The problem was that low priority tasks kept getting buried, if I wrote a TODO on a given date, it would disappear under newer entries within a week, and I had to manually scroll to make sure nothing was missed. The idea with `tl` is that I can keep using my log the same way, but every TODO gets tracked in a structured manner so no task gets lost regardless of how old it is. MCP support was added because managing tasks through a `PLAN.md` can get hard to review, and having an agent create and track tasks with brief notes across any project seemed like a better approach.

## How it works

`tl` treats your log file as the source of truth. It recognizes structured tasks that follow a strict pattern:

```
- [ ] tag-1 some task title
      - a note on this task
- [x] tag-2 a completed task
```

Everything else in the file (freeform bullets, prose, headers, links) is left untouched. Sections are separated by date headers:

```
### 12/02/2026
- [ ] dev-1 review the scan results
      - check the false positives
      - escalate critical findings

### 11/02/2026
- [x] infra-3 rotate production keys
```

Tags act as project identifiers. Each tag gets its own auto-incrementing counter, so `dev-1`, `dev-2`, `infra-1`, etc. are all independent.

## Installation

Requires Rust 1.88+.

```bash
cargo install --path .
```

This installs the `tl` binary to `~/.cargo/bin/`.

## Getting started

```bash
# Initialize with the default log location (~/.config/tasklog/log.md)
tl init

# Or point to your existing log file
tl init --log ~/notes/log.md
```

This creates:

- `~/.config/tasklog/config.toml` -- configuration
- `~/.config/tasklog/state.json` -- tag counters for ID allocation
- Your log file (created if it does not exist, left alone if it does)

## CLI usage

```bash
# Add a task
tl add dev implement the login flow
# => created dev-1

# Add another task under a different tag
tl add infra rotate production credentials
# => created infra-1

# Mark a task as done
tl done dev-1
# => completed dev-1

# Add a note to a task
tl note infra-1 blocked on access request
# => noted on infra-1

# Search across tasks and notes
tl search rotate
# => [x] infra-1 rotate production credentials

# Show today's raw section
tl today
```

## TUI

```bash
tl tui
```

The TUI has two panels:

- **Projects** (left) -- lists all tags with open/total task counts
- **Tasks** (right) -- shows tasks for the selected project

### Keybindings

| Key | Action |
|---|---|
| `j` / `k` | Navigate up/down |
| `h` / `l` | Switch to projects/tasks panel |
| `Tab` | Toggle panel focus |
| `Enter` | Open task detail popup |
| `Esc` | Close popup / quit |
| `a` | Add a new task (prompts for tag, then title) |
| `d` | Mark selected task as done |
| `n` | Add a note to selected task |
| `/` | Search |
| `c` | Clear search filter |
| `g` / `G` | Jump to top/bottom |
| `r` | Refresh from disk |
| `q` | Quit |

The task detail popup shows the full task ID, status, title, and all notes.

## MCP server

`tl` includes a [Model Context Protocol](https://modelcontextprotocol.io/) server so LLM-based tools can read and write your task log directly.

```bash
tl mcp
```

The server communicates over stdio and exposes these tools:

| Tool | Description |
|---|---|
| `init_log` | Initialize the task log environment |
| `create_task` | Create a new task with a tag and title |
| `complete_task` | Mark a task as completed by ID |
| `add_note` | Add a note to an existing task |
| `search_tasks` | Search tasks and notes, optionally filtered by tag |
| `get_today_section` | Get the raw text of today's section |

### Adding to your editor / tool

Most MCP-compatible tools accept a server definition like:

```json
{
  "tasklog": {
    "type": "local",
    "command": ["tl", "mcp"]
  }
}
```

For Claude Code, add it to your MCP config. For other tools (OpenCode, Continue, etc.), consult their MCP documentation for the exact config format. The only requirement is that `tl` is on your PATH.

### Testing the MCP server

You can use the MCP inspector to test interactively:

```bash
npx @modelcontextprotocol/inspector tl mcp
```

## Configuration

Config lives at `~/.config/tasklog/config.toml`:

```toml
log_path = "~/.config/tasklog/log.md"
date_format = "DD/MM/YYYY"
note_indent = 6
scan_window_lines = 5000
```

| Field | Description | Default |
|---|---|---|
| `log_path` | Path to your log file (supports `~`) | `~/.config/tasklog/log.md` |
| `date_format` | Date format for section headers | `DD/MM/YYYY` |
| `note_indent` | Number of spaces to indent notes | `6` |
| `scan_window_lines` | Only parse the last N lines of the log for performance | `5000` |

The key feature of `log_path` is that you can point it at an existing markdown file you already use. `tl` will add structured tasks alongside your freeform content without disturbing it.

## Design decisions

- **Single file** -- everything lives in one `log.md`. No databases, no hidden state beyond the counter file.
- **Overlay, not takeover** -- `tl` only reads and writes lines matching its strict task pattern. Your freeform markdown is invisible to it and never modified.
- **Global, not per-project** -- one log for everything, with tags to separate concerns. This matches how many people already keep a daily work log.
- **Atomic writes** -- all file mutations use write-to-temp then rename, so your log is never left in a half-written state.
- **File locking** -- concurrent CLI/TUI/MCP access is safe via `flock`.
- **Scan window** -- only the last N lines are parsed, so the tool stays fast even on large log files.

