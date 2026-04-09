use std::error::Error;
use rusqlite::{params, Connection};
use pulldown_cmark::{Event, Parser, Tag, Options};

pub fn import_csv(conn: &Connection, path: &str) -> Result<(), Box<dyn Error>> {
    let mut rdr = csv::ReaderBuilder::new()
        .flexible(true)
        .from_path(path)?;

    for result in rdr.records() {
        let record = result?;
        let title = match record.get(0) {
            Some(t) if !t.is_empty() => t,
            _ => continue,
        };
        let parent_title = record.get(1);

        let mut parent_id: Option<i32> = None;
        if let Some(p_title) = parent_title {
            if !p_title.is_empty() {
                parent_id = conn.query_row(
                    "SELECT id FROM tasks WHERE title = ?",
                    params![p_title],
                    |row| row.get(0),
                ).ok();
            }
        }

        conn.execute(
            "INSERT INTO tasks (list_id, parent_id, title) VALUES (?, ?, ?)",
            params![1, parent_id, title],
        )?;
    }
    Ok(())
}

pub fn import_markdown(conn: &Connection, path: &str) -> Result<(), Box<dyn Error>> {
    let content = std::fs::read_to_string(path)?;
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TASKLISTS);
    let parser = Parser::new_ext(&content, options);

    let mut stack: Vec<Option<i32>> = Vec::new();
    stack.push(None); // Root level

    let mut current_title = String::new();
    let mut has_pending = false;

    let flush_pending = |title: &mut String, has_p: &mut bool, conn: &Connection, parent_stack: &mut Vec<Option<i32>>| -> Result<Option<i32>, Box<dyn Error>> {
        if *has_p {
            let t = title.trim();
            if !t.is_empty() {
                let parent_id = parent_stack.last().copied().flatten();
                conn.execute(
                    "INSERT INTO tasks (list_id, parent_id, title) VALUES (1, ?, ?)",
                    params![parent_id, t],
                )?;
                let id = conn.last_insert_rowid() as i32;
                *title = String::new();
                *has_p = false;
                Ok(Some(id))
            } else {
                *title = String::new();
                *has_p = false;
                Ok(None)
            }
        } else {
            Ok(None)
        }
    };

    for event in parser {
        match event {
            Event::Start(Tag::List(_)) => {
                // If there's a pending item, it becomes the parent of this new list
                let new_parent = flush_pending(&mut current_title, &mut has_pending, conn, &mut stack)?;
                if new_parent.is_some() {
                    stack.push(new_parent);
                } else {
                    // If no pending item (e.g., at root, or list not preceded by item),
                    // the new list shares the same parent as the current level.
                    let current_parent = stack.last().copied().flatten();
                    stack.push(current_parent);
                }
            }
            Event::End(Tag::List(_)) => {
                stack.pop();
            }
            Event::Start(Tag::Item) => {
                // If there's already a pending item, it means it had no children (it's a sibling)
                flush_pending(&mut current_title, &mut has_pending, conn, &mut stack)?;
                has_pending = true;
            }
            Event::Text(t) => {
                if has_pending {
                    current_title.push_str(&t);
                }
            }
            Event::End(Tag::Item) => {
                // End of item. If it's still pending, it never had a nested list.
                flush_pending(&mut current_title, &mut has_pending, conn, &mut stack)?;
            }
            _ => {}
        }
    }
    
    // Catch any trailing item
    flush_pending(&mut current_title, &mut has_pending, conn, &mut stack)?;
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute("CREATE TABLE lists (id INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE)", []).unwrap();
        conn.execute("CREATE TABLE tasks (id INTEGER PRIMARY KEY, list_id INTEGER NOT NULL, parent_id INTEGER, title TEXT NOT NULL, completed INTEGER DEFAULT 0)", []).unwrap();
        conn.execute("INSERT INTO lists (name) VALUES ('General')", []).unwrap();
        conn
    }

    #[test]
    fn test_import_markdown_nested() {
        let conn = setup_test_db();
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "- [ ] Parent\n  - [ ] Child\n    - [ ] Grandchild").unwrap();
        file.flush().unwrap();

        import_markdown(&conn, file.path().to_str().unwrap()).unwrap();

        let mut stmt = conn.prepare("SELECT title, parent_id FROM tasks ORDER BY id").unwrap();
        let results: Vec<(String, Option<i32>)> = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(|r| r.unwrap()).collect();
        
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].0, "Parent");
        assert_eq!(results[1].0, "Child");
        assert_eq!(results[1].1, Some(1));
        assert_eq!(results[2].0, "Grandchild");
        assert_eq!(results[2].1, Some(2));
    }

    #[test]
    fn test_import_csv() {
        let conn = setup_test_db();
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "title,parent_title\nTask 1,\nTask 2,Task 1").unwrap();
        file.flush().unwrap();

        import_csv(&conn, file.path().to_str().unwrap()).unwrap();

        let mut stmt = conn.prepare("SELECT title, parent_id FROM tasks").unwrap();
        let results: Vec<(String, Option<i32>)> = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(|r| r.unwrap()).collect();
        assert_eq!(results.len(), 2);
    }
}
