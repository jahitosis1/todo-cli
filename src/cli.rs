use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "todo")]
#[command(about = "A robust CLI Todo application", long_about = None)]
#[command(arg_required_else_help = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Add a new task
    Add {
        /// The title of the task
        title: String,
        /// Optional parent task ID for subtasks
        #[arg(short, long)]
        parent: Option<i32>,
        /// Task priority (high, medium, low)
        #[arg(short = 'P', long)]
        priority: Option<String>,
        /// Task deadline (YYYY-MM-DD)
        #[arg(short, long)]
        deadline: Option<String>,
    },
    /// List all tasks
    List {
        /// Show only completed tasks
        #[arg(short, long)]
        completed: bool,

        /// Show tasks in a hierarchical tree view
        #[arg(short, long)]
        tree: bool,

        /// Use emojis in tree view
        #[arg(short, long)]
        emoji: bool,
    },
    /// Mark a task as done
    Done {
        /// The ID of the task to mark as done
        id: i32,
    },
    /// Delete a task
    Delete {
        /// The ID of the task to delete
        id: i32,
    },
    /// Create a new list
    ListCreate {
        /// The name of the new list
        name: String,
    },
    /// Import tasks from a file (.csv or .md)
    /// 
    /// CSV Template (title, parent_title):
    /// title,parent_title
    /// "Parent Task",
    /// "Subtask","Parent Task"
    /// 
    /// Markdown Template:
    /// # Todo List
    /// - [ ] Parent Task
    ///   - [ ] Subtask
    #[command(verbatim_doc_comment)]
    Import {
        /// Path to the CSV or Markdown file
        path: String,
    },
    /// Export tasks to a file (.csv or .md)
    /// 
    /// CSV Export includes: title, parent_title, priority, deadline, completed
    /// 
    /// Markdown Export produces a hierarchical list:
    /// # Todo List
    /// - [ ] Task name
    #[command(verbatim_doc_comment)]
    Export {
        /// Path to the output file
        path: String,
    },
    /// Manage the background daemon
    Daemon {
        #[command(subcommand)]
        action: DaemonAction,
    },
    /// Start the Terminal UI
    Tui,
    /// Mark a task as undone (incomplete)
    Undone {
        /// The ID of the task to mark as undone
        id: i32,
    },
}

#[derive(Subcommand)]
pub enum DaemonAction {
    /// Start the background notification daemon
    Start,
    /// Stop the background notification daemon
    Stop,
}
