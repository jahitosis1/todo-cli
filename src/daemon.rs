use crate::db;
use daemonize::Daemonize;
use notify_rust::Notification;
use rusqlite::Result;
use std::fs::File;
use std::thread;
use std::time::Duration;
use chrono::Local;

pub fn start() {
    let stdout = match File::create("/tmp/todo-daemon.out") {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Error creating stdout log: {}", e);
            return;
        }
    };
    let stderr = match File::create("/tmp/todo-daemon.err") {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Error creating stderr log: {}", e);
            return;
        }
    };

    let daemonize = Daemonize::new()
        .pid_file("/tmp/todo-daemon.pid")
        .stdout(stdout)
        .stderr(stderr);

    match daemonize.start() {
        Ok(_) => {
            // After successful daemonization, we are in the background process
            loop {
                if let Err(e) = check_deadlines() {
                    eprintln!("Error checking deadlines: {}", e);
                }
                thread::sleep(Duration::from_secs(60));
            }
        }
        Err(e) => eprintln!("Error starting daemon: {}", e),
    }
}

fn check_deadlines() -> Result<()> {
    let conn = db::init_db()?;
    let now = Local::now().format("%Y-%m-%d").to_string();

    let mut stmt = conn.prepare("SELECT id, title FROM tasks WHERE deadline = ? AND completed = 0")?;
    let rows = stmt.query_map([now], |row| {
        Ok((row.get::<_, i32>(0)?, row.get::<_, String>(1)?))
    })?;

    for row in rows {
        let (id, title) = row?;
        if let Err(e) = Notification::new()
            .summary("Task Deadline Today!")
            .body(&format!("Task {}: {}", id, title))
            .show()
        {
            eprintln!("Failed to show notification for task {}: {}", id, e);
        }
    }

    Ok(())
}

pub fn stop() {
    // Simple way to stop: read PID from file and kill process
    if let Ok(pid_str) = std::fs::read_to_string("/tmp/todo-daemon.pid") {
        if let Ok(pid) = pid_str.trim().parse::<i32>() {
            unsafe {
                libc::kill(pid, libc::SIGTERM);
            }
            println!("Daemon stopped.");
            let _ = std::fs::remove_file("/tmp/todo-daemon.pid");
        }
    } else {
        println!("Daemon not running.");
    }
}
