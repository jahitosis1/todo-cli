use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame, Terminal,
};
use rusqlite::{params, Connection, Result};
use std::io;

struct Task {
    id: i32,
    title: String,
    completed: i32,
    level: usize,
}

enum AppMode {
    Normal,
    InputAdd(Option<i32>), // parent_id
    InputRename(i32),      // task_id
    ConfirmDelete(i32),    // task_id
    ConfirmComplete(i32),  // task_id
    Search,
}

struct AppState {
    tasks: Vec<Task>,
    selected: usize,
    list_state: ListState,
    mode: AppMode,
    input: String,
    input_cursor: usize,
    search_query: String,
    g_pressed: bool,
}

impl AppState {
    fn new(conn: &Connection) -> Result<Self> {
        let mut app = AppState {
            tasks: Vec::new(),
            selected: 0,
            list_state: ListState::default(),
            mode: AppMode::Normal,
            input: String::new(),
            input_cursor: 0,
            search_query: String::new(),
            g_pressed: false,
        };
        app.reload_tasks(conn)?;
        Ok(app)
    }

    fn reload_tasks(&mut self, conn: &Connection) -> Result<()> {
        self.tasks.clear();

        let visible_ids = if !self.search_query.is_empty() {
            let mut stmt = conn.prepare(
                "WITH RECURSIVE
                    matches AS (
                        SELECT id FROM tasks WHERE title LIKE ?
                    ),
                    ancestors AS (
                        SELECT t.id, t.parent_id FROM tasks t JOIN matches m ON t.id = m.id
                        UNION ALL
                        SELECT t.id, t.parent_id FROM tasks t JOIN ancestors a ON t.id = a.parent_id
                    ),
                    descendants AS (
                        SELECT t.id, t.parent_id FROM tasks t JOIN matches m ON t.id = m.id
                        UNION ALL
                        SELECT t.id, t.parent_id FROM tasks t JOIN descendants d ON t.parent_id = d.id
                    ),
                    visible_ids AS (
                        SELECT id FROM ancestors
                        UNION
                        SELECT id FROM descendants
                    )
                SELECT id FROM visible_ids",
            )?;
            let query = format!("%{}%", self.search_query);
            let rows = stmt.query_map(params![query], |row| row.get::<_, i32>(0))?;
            let ids: std::collections::HashSet<i32> = rows.collect::<Result<_, _>>()?;
            Some(ids)
        } else {
            None
        };

        Self::load_tasks_recursive(conn, None, 0, &mut self.tasks, &visible_ids)?;
        if self.selected >= self.tasks.len() && !self.tasks.is_empty() {
            self.selected = self.tasks.len() - 1;
        }
        if !self.tasks.is_empty() {
            self.list_state.select(Some(self.selected));
        } else {
            self.list_state.select(None);
        }
        Ok(())
    }

    fn load_tasks_recursive(
        conn: &Connection,
        parent: Option<i32>,
        level: usize,
        dest: &mut Vec<Task>,
        visible_ids: &Option<std::collections::HashSet<i32>>,
    ) -> Result<()> {
        let mut stmt =
            conn.prepare("SELECT id, title, completed FROM tasks WHERE parent_id IS ? ORDER BY id")?;
        let rows = stmt.query_map(params![parent], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?;
        let tasks: Vec<(i32, String, i32)> = rows.collect::<Result<_, _>>()?;
        for (id, title, completed) in tasks {
            if let Some(ids) = visible_ids {
                if !ids.contains(&id) {
                    continue;
                }
            }
            dest.push(Task {
                id,
                title,
                completed,
                level,
            });
            Self::load_tasks_recursive(conn, Some(id), level + 1, dest, visible_ids)?;
        }
        Ok(())
    }

    fn next(&mut self) {
        if !self.tasks.is_empty() {
            self.selected = (self.selected + 1) % self.tasks.len();
            self.list_state.select(Some(self.selected));
        }
    }

    fn previous(&mut self) {
        if !self.tasks.is_empty() {
            if self.selected == 0 {
                self.selected = self.tasks.len() - 1;
            } else {
                self.selected -= 1;
            }
            self.list_state.select(Some(self.selected));
        }
    }
}

fn has_children(conn: &Connection, id: i32) -> Result<bool> {
    let mut stmt = conn.prepare("SELECT COUNT(*) FROM tasks WHERE parent_id = ?")?;
    let count: i64 = stmt.query_row(params![id], |row| row.get(0))?;
    Ok(count > 0)
}

fn delete_task_recursive(conn: &Connection, id: i32) -> Result<()> {
    // Find children
    let mut stmt = conn.prepare("SELECT id FROM tasks WHERE parent_id = ?")?;
    let children: Vec<i32> = stmt
        .query_map(params![id], |row| row.get(0))?
        .collect::<Result<Vec<i32>, _>>()?;

    for child_id in children {
        delete_task_recursive(conn, child_id)?;
    }

    conn.execute("DELETE FROM tasks WHERE id = ?", params![id])?;
    Ok(())
}

fn mark_done_recursive(conn: &Connection, id: i32, status: i32) -> Result<()> {
    // Find children
    let mut stmt = conn.prepare("SELECT id FROM tasks WHERE parent_id = ?")?;
    let children: Vec<i32> = stmt
        .query_map(params![id], |row| row.get(0))?
        .collect::<Result<Vec<i32>, _>>()?;

    for child_id in children {
        mark_done_recursive(conn, child_id, status)?;
    }

    conn.execute("UPDATE tasks SET completed = ? WHERE id = ?", params![status, id])?;
    Ok(())
}

fn mark_parent_undone_recursive(conn: &Connection, child_id: i32) -> Result<()> {
    let parent_id: Option<i32> = conn
        .query_row(
            "SELECT parent_id FROM tasks WHERE id = ?",
            params![child_id],
            |row| row.get(0),
        )
        .ok()
        .flatten();

    if let Some(pid) = parent_id {
        conn.execute("UPDATE tasks SET completed = 0 WHERE id = ?", params![pid])?;
        mark_parent_undone_recursive(conn, pid)?;
    }
    Ok(())
}

fn check_and_update_parent_done_recursive(conn: &Connection, child_id: i32) -> Result<()> {
    let parent_id: Option<i32> = conn
        .query_row(
            "SELECT parent_id FROM tasks WHERE id = ?",
            params![child_id],
            |row| row.get(0),
        )
        .ok()
        .flatten();

    if let Some(pid) = parent_id {
        // Check if all siblings are done
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM tasks WHERE parent_id = ? AND completed = 0",
            params![pid],
            |row| row.get(0),
        )?;

        if count == 0 {
            conn.execute("UPDATE tasks SET completed = 1 WHERE id = ?", params![pid])?;
            check_and_update_parent_done_recursive(conn, pid)?;
        }
    }
    Ok(())
}





pub fn run(conn: &Connection) -> Result<()> {
    enable_raw_mode().unwrap();
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture).unwrap();
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).unwrap();

    let mut app = AppState::new(conn)?;

    let res = run_app(&mut terminal, &mut app, conn);

    disable_raw_mode().unwrap();
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )
    .unwrap();
    terminal.show_cursor().unwrap();

    if let Err(err) = res {
        println!("{:?}", err);
    }
    Ok(())
}

fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    app: &mut AppState,
    conn: &Connection,
) -> io::Result<()>
where
    std::io::Error: From<<B as ratatui::backend::Backend>::Error>,
{
    loop {
        terminal.draw(|f| ui(f, app))?;

        if let Event::Key(key) = event::read()? {
            match app.mode {
                AppMode::Normal => match key.code {
                    KeyCode::Char('q') => return Ok(()),
                    KeyCode::Esc => {
                        app.g_pressed = false;
                        if !app.search_query.is_empty() {
                            app.search_query.clear();
                            let _ = app.reload_tasks(conn);
                        } else {
                            return Ok(());
                        }
                    }
                    KeyCode::Char('j') | KeyCode::Down => {
                        app.g_pressed = false;
                        app.next();
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        app.g_pressed = false;
                        app.previous();
                    }
                    KeyCode::Char('g') => {
                        if app.g_pressed {
                            if !app.tasks.is_empty() {
                                app.selected = 0;
                                app.list_state.select(Some(app.selected));
                            }
                            app.g_pressed = false;
                        } else {
                            app.g_pressed = true;
                        }
                    }
                    KeyCode::Char('G') => {
                        app.g_pressed = false;
                        if !app.tasks.is_empty() {
                            app.selected = app.tasks.len() - 1;
                            app.list_state.select(Some(app.selected));
                        }
                    }
                    KeyCode::Char('/') => {
                        app.g_pressed = false;
                        app.mode = AppMode::Search;
                        app.input.clear();
                        app.input_cursor = 0;
                    }
                    KeyCode::Char('a') => {
                        app.g_pressed = false;
                        if !app.tasks.is_empty() {
                            let parent_id = app.tasks[app.selected].id;
                            app.mode = AppMode::InputAdd(Some(parent_id));
                            app.input.clear();
                            app.input_cursor = 0;
                        }
                    }
                    KeyCode::Char('A') => {
                        app.g_pressed = false;
                        app.mode = AppMode::InputAdd(None);
                        app.input.clear();
                        app.input_cursor = 0;
                    }
                    KeyCode::Char('r') => {
                        app.g_pressed = false;
                        if !app.tasks.is_empty() {
                            let task_id = app.tasks[app.selected].id;
                            app.mode = AppMode::InputRename(task_id);
                            app.input = app.tasks[app.selected].title.clone();
                            app.input_cursor = app.input.len();
                        }
                    }
                    KeyCode::Char('d') | KeyCode::Delete => {
                        app.g_pressed = false;
                        if !app.tasks.is_empty() {
                            let task_id = app.tasks[app.selected].id;
                            app.mode = AppMode::ConfirmDelete(task_id);
                        }
                    }
                    KeyCode::Char('x') | KeyCode::Char(' ') => {
                        app.g_pressed = false;
                        if !app.tasks.is_empty() {
                            let task = &app.tasks[app.selected];
                            let task_id = task.id;
                            let new_status = if task.completed == 1 { 0 } else { 1 };
                            
                            if new_status == 1 {
                                match has_children(conn, task_id) {
                                    Ok(true) => {
                                        app.mode = AppMode::ConfirmComplete(task_id);
                                    }
                                    _ => {
                                        let _ = conn.execute(
                                            "UPDATE tasks SET completed = 1 WHERE id = ?",
                                            params![task_id],
                                        );
                                        let _ = check_and_update_parent_done_recursive(conn, task_id);
                                        let _ = app.reload_tasks(conn);
                                    }
                                }
                            } else {
                                // Marking as undone marks ancestors as undone as well
                                let _ = conn.execute(
                                    "UPDATE tasks SET completed = 0 WHERE id = ?",
                                    params![task_id],
                                );
                                let _ = mark_parent_undone_recursive(conn, task_id);
                                let _ = app.reload_tasks(conn);
                            }
                        }
                    }
                    _ => {}
                },
                AppMode::InputAdd(parent_id) => match key.code {
                    KeyCode::Enter => {
                        if !app.input.is_empty() {
                            let _ = conn.execute(
                                "INSERT INTO tasks (list_id, parent_id, title) VALUES (1, ?, ?)",
                                params![parent_id, app.input],
                            );
                            if let Some(pid) = parent_id {
                                let _ = mark_parent_undone_recursive(conn, pid);
                            }
                        }
                        app.mode = AppMode::Normal;
                        app.input_cursor = 0;
                        let _ = app.reload_tasks(conn);
                    }
                    KeyCode::Char(c) => {
                        app.input.insert(app.input_cursor, c);
                        app.input_cursor += 1;
                    }
                    KeyCode::Backspace => {
                        if app.input_cursor > 0 {
                            app.input.remove(app.input_cursor - 1);
                            app.input_cursor -= 1;
                        }
                    }
                    KeyCode::Delete => {
                        if app.input_cursor < app.input.len() {
                            app.input.remove(app.input_cursor);
                        }
                    }
                    KeyCode::Left => {
                        if app.input_cursor > 0 {
                            app.input_cursor -= 1;
                        }
                    }
                    KeyCode::Right => {
                        if app.input_cursor < app.input.len() {
                            app.input_cursor += 1;
                        }
                    }
                    KeyCode::Home => {
                        app.input_cursor = 0;
                    }
                    KeyCode::End => {
                        app.input_cursor = app.input.len();
                    }
                    KeyCode::Esc => {
                        app.mode = AppMode::Normal;
                        app.input_cursor = 0;
                    }
                    _ => {}
                },
                AppMode::InputRename(task_id) => match key.code {
                    KeyCode::Enter => {
                        if !app.input.is_empty() {
                            let _ = conn.execute(
                                "UPDATE tasks SET title = ? WHERE id = ?",
                                params![app.input, task_id],
                            );
                        }
                        app.mode = AppMode::Normal;
                        app.input_cursor = 0;
                        let _ = app.reload_tasks(conn);
                    }
                    KeyCode::Char(c) => {
                        app.input.insert(app.input_cursor, c);
                        app.input_cursor += 1;
                    }
                    KeyCode::Backspace => {
                        if app.input_cursor > 0 {
                            app.input.remove(app.input_cursor - 1);
                            app.input_cursor -= 1;
                        }
                    }
                    KeyCode::Delete => {
                        if app.input_cursor < app.input.len() {
                            app.input.remove(app.input_cursor);
                        }
                    }
                    KeyCode::Left => {
                        if app.input_cursor > 0 {
                            app.input_cursor -= 1;
                        }
                    }
                    KeyCode::Right => {
                        if app.input_cursor < app.input.len() {
                            app.input_cursor += 1;
                        }
                    }
                    KeyCode::Home => {
                        app.input_cursor = 0;
                    }
                    KeyCode::End => {
                        app.input_cursor = app.input.len();
                    }
                    KeyCode::Esc => {
                        app.mode = AppMode::Normal;
                        app.input_cursor = 0;
                    }
                    _ => {}
                },
                AppMode::ConfirmDelete(task_id) => match key.code {
                    KeyCode::Char('y') | KeyCode::Char('Y') => {
                        let _ = delete_task_recursive(conn, task_id);
                        app.mode = AppMode::Normal;
                        let _ = app.reload_tasks(conn);
                    }
                    KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                        app.mode = AppMode::Normal;
                    }
                    _ => {}
                },
                AppMode::ConfirmComplete(task_id) => match key.code {
                    KeyCode::Char('y') | KeyCode::Char('Y') => {
                        let _ = mark_done_recursive(conn, task_id, 1);
                        let _ = check_and_update_parent_done_recursive(conn, task_id);
                        app.mode = AppMode::Normal;
                        let _ = app.reload_tasks(conn);
                    }
                    KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                        app.mode = AppMode::Normal;
                    }
                    _ => {}
                },
                AppMode::Search => match key.code {
                    KeyCode::Enter | KeyCode::Esc => {
                        app.mode = AppMode::Normal;
                        app.search_query = app.input.clone();
                        app.input.clear();
                        app.input_cursor = 0;
                        let _ = app.reload_tasks(conn);
                    }
                    KeyCode::Char(c) => {
                        app.input.insert(app.input_cursor, c);
                        app.input_cursor += 1;
                        app.search_query = app.input.clone();
                        let _ = app.reload_tasks(conn);
                    }
                    KeyCode::Backspace => {
                        if app.input_cursor > 0 {
                            app.input.remove(app.input_cursor - 1);
                            app.input_cursor -= 1;
                            app.search_query = app.input.clone();
                            let _ = app.reload_tasks(conn);
                        }
                    }
                    KeyCode::Delete => {
                        if app.input_cursor < app.input.len() {
                            app.input.remove(app.input_cursor);
                            app.search_query = app.input.clone();
                            let _ = app.reload_tasks(conn);
                        }
                    }
                    KeyCode::Left => {
                        if app.input_cursor > 0 {
                            app.input_cursor -= 1;
                        }
                    }
                    KeyCode::Right => {
                        if app.input_cursor < app.input.len() {
                            app.input_cursor += 1;
                        }
                    }
                    KeyCode::Home => {
                        app.input_cursor = 0;
                    }
                    KeyCode::End => {
                        app.input_cursor = app.input.len();
                    }
                    _ => {}
                },
            }
        }
    }
}

fn ui(f: &mut Frame, app: &mut AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints(
            [
                Constraint::Min(3),
                Constraint::Length(3),
            ]
            .as_ref(),
        )
        .split(f.area());

    let items: Vec<ListItem> = app
        .tasks
        .iter()
        .enumerate()
        .map(|(i, task)| {
            let is_selected = i == app.selected;
            let status = if task.completed == 1 { "[X]" } else { "[ ]" };
            let prefix = "  ".repeat(task.level);

            let mut line_spans = Vec::new();

            // Style for the checkbox and indentation
            let meta_style = if is_selected {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else if task.completed == 1 {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default().fg(Color::Gray)
            };

            line_spans.push(Span::styled(format!("{}{} ", prefix, status), meta_style));

            // Style for the title
            let title_style = if is_selected {
                Style::default()
                    .fg(Color::White)
                    .bg(Color::Indexed(237)) // Dark gray background
                    .add_modifier(Modifier::BOLD)
            } else if task.completed == 1 {
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::CROSSED_OUT)
            } else {
                Style::default().fg(Color::White)
            };

            line_spans.push(Span::styled(task.title.clone(), title_style));

            ListItem::new(Line::from(line_spans))
        })
        .collect();

    let (title, style) = match app.mode {
        AppMode::Normal => (
            " Tasks (a: subtask, A: main task, r: rename, d: delete, x/space: toggle, q: quit) ",
            Style::default(),
        ),
        AppMode::InputAdd(_) => (" Add Task (Esc to cancel, Enter to save) ", Style::default().fg(Color::Yellow)),
        AppMode::InputRename(_) => (" Rename Task (Esc to cancel, Enter to save) ", Style::default().fg(Color::Yellow)),
        AppMode::ConfirmDelete(_) => (
            " CONFIRM DELETE? (y/n) All tasks and subtasks will be removed! ",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
        AppMode::ConfirmComplete(_) => (
            " CONFIRM DONE? (y/n) All subtasks will be marked as done! ",
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
        ),
        AppMode::Search => (
            " Search / Filter (Enter to finish, Esc to clear) ",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
    };

    let tasks_block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(style);

    let list = List::new(items).block(tasks_block);
    f.render_stateful_widget(list, chunks[0], &mut app.list_state);

    if let AppMode::InputAdd(_) | AppMode::InputRename(_) | AppMode::Search = app.mode {
        let input_title = match app.mode {
            AppMode::InputAdd(_) => " New Task Title (Enter to save, Esc to cancel) ",
            AppMode::InputRename(_) => " Rename Task (Enter to save, Esc to cancel) ",
            AppMode::Search => " Search Query (Enter/Esc to finish) ",
            _ => "",
        };
        let input_block = Block::default()
            .borders(Borders::ALL)
            .title(input_title);
        let input_text = Paragraph::new(app.input.clone()).block(input_block);
        f.render_widget(input_text, chunks[1]);
        
        f.set_cursor_position((
            chunks[1].x + app.input_cursor as u16 + 1,
            chunks[1].y + 1,
        ));
    } else if let AppMode::ConfirmDelete(_) = app.mode {
        let help_text = Paragraph::new(
            "   !!! WARNING: This will delete the task and ALL its nested subtasks. Press 'y' to confirm or 'n' to cancel. !!!",
        )
        .style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))
        .block(Block::default().borders(Borders::ALL).title(" Danger Zone "));
        f.render_widget(help_text, chunks[1]);
    } else if let AppMode::ConfirmComplete(_) = app.mode {
        let help_text = Paragraph::new(
            "   >>> MARK AS DONE? Press 'y' to confirm marking all subtasks as complete or 'n' to cancel. <<<",
        )
        .style(Style::default().fg(Color::Green).add_modifier(Modifier::BOLD))
        .block(Block::default().borders(Borders::ALL).title(" Bulk Action "));
        f.render_widget(help_text, chunks[1]);
    } else {
        let help_text = if !app.search_query.is_empty() {
            Paragraph::new(format!(
                " FILTER: '{}' | Nav: j/k, gg/G | Delete: d | Search: / | Esc: Clear Filter ",
                app.search_query
            ))
        } else {
            Paragraph::new(
                " Nav: j/k, gg/G | Subtask/Task: a/A | Rename: r | Delete: d | Toggle: x/Space | Search: / | Quit: q ",
            )
        };
        f.render_widget(
            help_text
                .style(Style::default().fg(Color::DarkGray))
                .block(Block::default().borders(Borders::ALL).title(" Legend ")),
            chunks[1],
        );
    }
}
