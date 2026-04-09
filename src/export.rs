use csv::WriterBuilder;
use std::error::Error;
use rusqlite::Connection;
use std::fs::File;
use std::io::Write;

pub fn export_csv(conn: &Connection, path: &str) -> Result<(), Box<dyn Error>> {
    let mut wtr = WriterBuilder::new().from_path(path)?;
    wtr.write_record(&["title", "parent_title", "priority", "deadline", "completed"])?;

    let mut stmt = conn.prepare("
        SELECT t.title, p.title, t.priority, t.deadline, t.completed 
        FROM tasks t 
        LEFT JOIN tasks p ON t.parent_id = p.id
    ")?;

    let rows = stmt.query_map([], |row| {
        let title: String = row.get(0)?;
        let parent_title: Option<String> = row.get(1)?;
        let priority: Option<String> = row.get(2)?;
        let deadline: Option<String> = row.get(3)?;
        let completed: i32 = row.get(4)?;
        Ok((title, parent_title.unwrap_or_default(), priority.unwrap_or_default(), deadline.unwrap_or_default(), completed))
    })?;

    for row in rows {
        let (title, parent_title, priority, deadline, completed) = row?;
        wtr.write_record(&[title, parent_title, priority, deadline, completed.to_string()])?;
    }

    wtr.flush()?;
    Ok(())
}

pub fn export_markdown(conn: &Connection, path: &str) -> Result<(), Box<dyn Error>> {
    let mut file = File::create(path)?;
    writeln!(file, "# Todo List\n")?;

    // We use a recursive approach to handle hierarchy
    write_tasks_recursive(conn, None, 0, &mut file)?;

    Ok(())
}

fn write_tasks_recursive(conn: &Connection, parent_id: Option<i32>, indent: usize, file: &mut File) -> Result<(), Box<dyn Error>> {
    let mut stmt = if parent_id.is_some() {
        conn.prepare("SELECT id, title, completed FROM tasks WHERE parent_id = ?")?
    } else {
        conn.prepare("SELECT id, title, completed FROM tasks WHERE parent_id IS NULL")?
    };

    let item_mapper = |row: &rusqlite::Row| {
        Ok((row.get::<_, i32>(0)?, row.get::<_, String>(1)?, row.get::<_, i32>(2)?))
    };

    let mut rows = if let Some(pid) = parent_id {
        stmt.query_map([pid], item_mapper)?
    } else {
        stmt.query_map([], item_mapper)?
    };

    while let Some(result) = rows.next() {
        let (id, title, completed) = result?;
        let marker = if completed == 1 { "[x]" } else { "[ ]" };
        let indentation = "  ".repeat(indent);
        writeln!(file, "{}- {} {}", indentation, marker, title)?;
        
        // Recursive call for subtasks
        write_tasks_recursive(conn, Some(id), indent + 1, file)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use tempfile::NamedTempFile;
    use std::io::Read;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute("CREATE TABLE tasks (id INTEGER PRIMARY KEY, list_id INTEGER, parent_id INTEGER, title TEXT, priority TEXT, deadline TEXT, completed INTEGER DEFAULT 0)", []).unwrap();
        conn.execute("INSERT INTO tasks (list_id, title, priority, completed) VALUES (1, 'Task A', 'high', 0)", []).unwrap();
        let aid: i32 = conn.last_insert_rowid() as i32;
        conn.execute("INSERT INTO tasks (list_id, parent_id, title, completed) VALUES (1, ?, 'Subtask B', 1)", [aid]).unwrap();
        conn
    }

    #[test]
    fn test_export_csv() {
        let conn = setup_test_db();
        let file = NamedTempFile::new().unwrap();
        let path = file.path().to_str().unwrap();

        export_csv(&conn, path).unwrap();

        let mut content = String::new();
        File::open(path).expect("failed to open exported CSV").read_to_string(&mut content).unwrap();
        assert!(content.contains("Task A"));
        assert!(content.contains("Subtask B"));
        assert!(content.contains("high"));
    }

    #[test]
    fn test_export_markdown() {
        let conn = setup_test_db();
        let file = NamedTempFile::new().unwrap();
        let path = file.path().to_str().unwrap();

        export_markdown(&conn, path).unwrap();

        let mut content = String::new();
        File::open(path).expect("failed to open exported MD").read_to_string(&mut content).unwrap();
        assert!(content.contains("- [ ] Task A"));
        assert!(content.contains("  - [x] Subtask B"));
    }
}
