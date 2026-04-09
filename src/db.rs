use rusqlite::{Connection, Result};
use std::path::PathBuf;
use directories::ProjectDirs;

pub fn get_db_path() -> PathBuf {
    let proj_dirs = ProjectDirs::from("com", "joshua", "todo-cli")
        .expect("Could not determine project directories");
    let config_dir = proj_dirs.config_dir();
    std::fs::create_dir_all(config_dir).expect("Could not create config directory");
    config_dir.join("todo.db")
}

pub fn init_db() -> Result<Connection> {
    let conn = Connection::open(get_db_path())?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS lists (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL UNIQUE
        )",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS tasks (
            id INTEGER PRIMARY KEY,
            list_id INTEGER NOT NULL,
            parent_id INTEGER,
            title TEXT NOT NULL,
            description TEXT,
            priority TEXT,
            deadline TEXT,
            completed INTEGER DEFAULT 0,
            FOREIGN KEY(list_id) REFERENCES lists(id),
            FOREIGN KEY(parent_id) REFERENCES tasks(id)
        )",
        [],
    )?;

    // Insert a default list if none exists
    conn.execute(
        "INSERT OR IGNORE INTO lists (name) VALUES ('General')",
        [],
    )?;

    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_duplicate_list_name() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute("CREATE TABLE lists (id INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE)", []).unwrap();
        conn.execute("INSERT INTO lists (name) VALUES ('General')", []).unwrap();
        let res = conn.execute("INSERT INTO lists (name) VALUES ('General')", []);
        assert!(res.is_err());
    }

    #[test]
    fn test_foreign_key_constraint() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute("PRAGMA foreign_keys = ON", []).unwrap();
        conn.execute("CREATE TABLE lists (id INTEGER PRIMARY KEY, name TEXT)", []).unwrap();
        conn.execute("CREATE TABLE tasks (id INTEGER PRIMARY KEY, list_id INTEGER, FOREIGN KEY(list_id) REFERENCES lists(id))", []).unwrap();
        
        // Attempt to insert task with non-existent list_id
        let res = conn.execute("INSERT INTO tasks (list_id) VALUES (99)", []);
        assert!(res.is_err());
    }
}
