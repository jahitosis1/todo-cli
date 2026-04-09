mod db;
mod cli;
mod import;
mod export;
mod daemon;
mod tui;

use std::io::Write;
use clap::Parser;
use cli::{Cli, Commands, DaemonAction};
use rusqlite::{params, Result};

fn main() -> Result<()> {
    let cli = Cli::parse();
    let conn = db::init_db()?;
    let mut stdout = std::io::stdout();

    match cli.command {
        Commands::Add { title, parent, priority, deadline } => {
            conn.execute(
                "INSERT INTO tasks (list_id, parent_id, title, priority, deadline) VALUES (?, ?, ?, ?, ?)",
                params![1, parent, title, priority, deadline],
            )?;
            writeln!(stdout, "Task added successfully.").unwrap();
        }
        Commands::List { completed, tree, emoji } => {
            if tree {
                print_tasks_tree(&conn, None, 0, emoji, &mut stdout)?;
            } else {
                print_tasks(&conn, None, 0, completed, &mut stdout)?;
            }
        }
        Commands::Done { id } => {
            conn.execute("UPDATE tasks SET completed = 1 WHERE id = ?", [id])?;
            writeln!(stdout, "Task {} marked as done.", id).unwrap();
        }
        Commands::Delete { id } => {
            conn.execute("DELETE FROM tasks WHERE id = ?", [id])?;
            writeln!(stdout, "Task {} deleted.", id).unwrap();
        }
        Commands::Import { path } => {
            let result = if path.ends_with(".csv") {
                import::import_csv(&conn, &path)
            } else if path.ends_with(".md") {
                import::import_markdown(&conn, &path)
            } else {
                writeln!(stdout, "Error: Unsupported file format. Please use .csv or .md").unwrap();
                return Ok(());
            };

            match result {
                Ok(_) => writeln!(stdout, "Import successful.").unwrap(),
                Err(e) => eprintln!("Error importing file: {}", e),
            }
        }
        Commands::Export { path } => {
            let result = if path.ends_with(".csv") {
                export::export_csv(&conn, &path)
            } else if path.ends_with(".md") {
                export::export_markdown(&conn, &path)
            } else {
                writeln!(stdout, "Error: Unsupported file format. Please use .csv or .md").unwrap();
                return Ok(());
            };

            match result {
                Ok(_) => writeln!(stdout, "Export successful context saved to: {}", path).unwrap(),
                Err(e) => eprintln!("Error exporting file: {}", e),
            }
        }
        Commands::Daemon { action } => match action {
            DaemonAction::Start => daemon::start(),
            DaemonAction::Stop => daemon::stop(),
        },
        Commands::Undone { id } => {
            conn.execute("UPDATE tasks SET completed = 0 WHERE id = ?", [id])?;
            writeln!(stdout, "Task {} marked as undone.", id).unwrap();
        }
        Commands::Tui => {
            tui::run(&conn)?;
        }
        _ => {
            println!("Command not yet implemented.");
        }
    }

    Ok(())
}

fn print_tasks(conn: &rusqlite::Connection, parent_id: Option<i32>, indent: usize, completed: bool, w: &mut dyn Write) -> Result<()> {
    let mut stmt = conn.prepare("SELECT id, title, completed FROM tasks WHERE parent_id IS ? AND completed = ?")?;
    let rows = stmt.query_map(params![parent_id, if completed { 1 } else { 0 }], |row| {
        Ok((row.get::<_, i32>(0)?, row.get::<_, String>(1)?, row.get::<_, i32>(2)?))
    })?;

    let tasks: Vec<(i32, String, i32)> = rows.collect::<std::result::Result<_, _>>()?;

    if indent == 0 && !tasks.is_empty() {
        writeln!(w, "{:<5} {:<30} {:<10}", "ID", "Title", "Status").unwrap();
    }

    for (id, title, status) in tasks {
        let status_str = if status == 1 { "Done" } else { "Pending" };
        writeln!(w, "{:<5} {:<width$} {:<10}", id, format!("{}{}", "  ".repeat(indent), title), status_str, width = 30).unwrap();
        print_tasks(conn, Some(id), indent + 1, completed, w)?;
    }
    Ok(())
}

fn print_tasks_tree(conn: &rusqlite::Connection, parent_id: Option<i32>, indent: usize, use_emoji: bool, w: &mut dyn Write) -> Result<()> {
    let mut stmt = conn.prepare("SELECT id, title, completed FROM tasks WHERE parent_id IS ?")?;
    let rows = stmt.query_map(params![parent_id], |row| {
        Ok((row.get::<_, i32>(0)?, row.get::<_, String>(1)?, row.get::<_, i32>(2)?))
    })?;

    let tasks: Vec<(i32, String, i32)> = rows.collect::<std::result::Result<_, _>>()?;

    for (id, title, status) in tasks {
        let status_mark = if use_emoji {
            if status == 1 { "✅" } else { "❌" }
        } else {
            if status == 1 { "[x]" } else { "[ ]" }
        };
        
        let indentation = if indent > 0 {
            format!("{}└── ", "  ".repeat(indent - 1))
        } else {
            String::new()
        };
        
        writeln!(w, "{} {} (ID: {}) {}", indentation, status_mark, id, title).unwrap();
        print_tasks_tree(conn, Some(id), indent + 1, use_emoji, w)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute("CREATE TABLE tasks (id INTEGER PRIMARY KEY, list_id INTEGER, parent_id INTEGER, title TEXT, priority TEXT, deadline TEXT, completed INTEGER DEFAULT 0)", []).unwrap();
        conn.execute("INSERT INTO tasks (list_id, title, priority, completed) VALUES (1, 'Parent', 'high', 0)", []).unwrap();
        let pid: i32 = conn.last_insert_rowid() as i32;
        conn.execute("INSERT INTO tasks (list_id, parent_id, title, completed) VALUES (1, ?, 'Child', 1)", [pid]).unwrap();
        conn
    }

    #[test]
    fn test_print_tasks_tree_plain() {
        let conn = setup_test_db();
        let mut buf = Vec::new();
        print_tasks_tree(&conn, None, 0, false, &mut buf).unwrap();
        let output = String::from_utf8(buf).unwrap();
        println!("Output: {:?}", output);
        assert!(output.contains("[ ] (ID: 1) Parent"));
        assert!(output.contains("└──  [x] (ID: 2) Child"));
    }

    #[test]
    fn test_print_tasks_tree_emoji() {
        let conn = setup_test_db();
        let mut buf = Vec::new();
        print_tasks_tree(&conn, None, 0, true, &mut buf).unwrap();
        let output = String::from_utf8(buf).unwrap();
        println!("Output: {:?}", output);
        assert!(output.contains("❌ (ID: 1) Parent"));
        assert!(output.contains("└──  ✅ (ID: 2) Child"));
    }
}
