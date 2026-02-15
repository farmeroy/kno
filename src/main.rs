use std::env;
use std::fs;
use std::path::PathBuf;
use std::process;

use chrono::Local;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(about = "A simple notes CLI", args_conflicts_with_subcommands = true)]
struct Cli {
    /// Note path (e.g. sql/joins). Opens daily note if omitted.
    path: Option<String>,

    /// Text to append to the note. Appends and exits without opening editor.
    #[arg(short, long, allow_hyphen_values = true)]
    append: Option<String>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// List note topics and directories
    List {
        /// Directory to list (lists all if omitted)
        path: Option<String>,

        /// Show individual notes, not just directories
        #[arg(short, long)]
        notes: bool,
    },

    /// Run git commands in the notes directory
    #[command(trailing_var_arg = true)]
    Git {
        /// Arguments to pass to git
        args: Vec<String>,
    },
}

fn titlecase(s: &str) -> String {
    s.split(|c| c == '-' || c == '_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => {
                    let upper: String = c.to_uppercase().collect();
                    upper + chars.as_str()
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn resolve_note(path: Option<&str>) -> (PathBuf, String) {
    let today = Local::now().format("%Y-%m-%d").to_string();

    match path {
        None => {
            // Default: daily directory with year grouping
            let parts: Vec<&str> = today.splitn(2, '-').collect();
            let year = parts[0];
            let rest = parts[1]; // MM-DD
            let path = PathBuf::from("daily").join(year).join(format!("{rest}.md"));
            let header = format!("# {today}");
            (path, header)
        }
        Some(note_path) if note_path.ends_with('/') => {
            // Trailing slash: treat as directory, use today's date as filename
            let path = PathBuf::from(note_path).join(format!("{today}.md"));
            let header = format!("# {today}");
            (path, header)
        }
        Some(note_path) => {
            // Explicit note name
            let path = PathBuf::from(format!("{note_path}.md"));
            let stem = path.file_stem().unwrap().to_string_lossy();
            let header = format!("# {}", titlecase(&stem));
            (path, header)
        }
    }
}

fn open_note(notes_dir: &std::path::Path, path: Option<&str>) -> PathBuf {
    let (relative_path, header) = resolve_note(path);
    let file_path = notes_dir.join(&relative_path);

    if let Some(parent) = file_path.parent() {
        fs::create_dir_all(parent).expect("failed to create note directory");
    }

    let needs_header = !file_path.exists()
        || fs::read_to_string(&file_path)
            .map(|c| c.trim().is_empty())
            .unwrap_or(true);

    if needs_header {
        fs::write(&file_path, format!("{header}\n\n")).expect("failed to write note file");
    }

    file_path
}

fn append_to_note(file_path: &std::path::Path, text: &str) {
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(file_path)
        .expect("failed to open note for appending");
    writeln!(file, "{text}").expect("failed to append to note");
}

fn list_tree(dir: &std::path::Path, prefix: &str, show_notes: bool, output: &mut String) {
    let mut entries: Vec<_> = match fs::read_dir(dir) {
        Ok(rd) => rd.filter_map(|e| e.ok()).collect(),
        Err(_) => return,
    };
    entries.sort_by_key(|e| e.file_name());

    // Filter: dirs always, files only if show_notes and .md extension
    let entries: Vec<_> = entries
        .into_iter()
        .filter(|e| {
            let name = e.file_name();
            let name_str = name.to_string_lossy();
            let ft = e.file_type().unwrap();
            if ft.is_dir() {
                !name_str.starts_with('.')
            } else {
                show_notes && e.path().extension().is_some_and(|ext| ext == "md")
            }
        })
        .collect();

    for (i, entry) in entries.iter().enumerate() {
        let is_last = i == entries.len() - 1;
        let connector = if is_last { "└── " } else { "├── " };
        let name = entry.file_name();
        let display_name = if entry.file_type().unwrap().is_file() {
            // Strip .md extension for display
            let n = name.to_string_lossy();
            n.strip_suffix(".md").unwrap_or(&n).to_string()
        } else {
            name.to_string_lossy().to_string()
        };

        output.push_str(&format!("{prefix}{connector}{display_name}\n"));

        if entry.file_type().unwrap().is_dir() {
            let child_prefix = if is_last {
                format!("{prefix}    ")
            } else {
                format!("{prefix}│   ")
            };
            list_tree(&entry.path(), &child_prefix, show_notes, output);
        }
    }
}

fn list_notes(notes_dir: &std::path::Path, path: Option<&str>, show_notes: bool) -> String {
    let root = match path {
        Some(p) => notes_dir.join(p),
        None => notes_dir.to_path_buf(),
    };

    if !root.is_dir() {
        return format!("{} is not a directory\n", root.display());
    }

    let label = path.unwrap_or(".");
    let mut output = format!("{label}\n");
    list_tree(&root, "", show_notes, &mut output);
    output
}

fn run_git(notes_dir: &std::path::Path, args: &[String]) {
    let is_init = args.first().is_some_and(|a| a == "init");

    if !is_init && !notes_dir.join(".git").exists() {
        eprintln!("Notes directory is not a git repo. Run `kno git init` to initialize.");
        process::exit(1);
    }

    let status = process::Command::new("git")
        .arg("-C")
        .arg(notes_dir)
        .args(args)
        .status()
        .expect("failed to run git");

    process::exit(status.code().unwrap_or(1));
}

fn main() {
    let cli = Cli::parse();

    let home = env::var("HOME").expect("HOME not set");
    let notes_dir = PathBuf::from(&home).join(".notes");

    match &cli.command {
        Some(Command::Git { args }) => {
            run_git(&notes_dir, args);
        }
        Some(Command::List { path, notes }) => {
            let output = list_notes(&notes_dir, path.as_deref(), *notes);
            print!("{output}");
            return;
        }
        None => {}
    }

    let file_path = open_note(&notes_dir, cli.path.as_deref());

    if let Some(text) = &cli.append {
        append_to_note(&file_path, text);
        return;
    }

    let editor = env::var("EDITOR").unwrap_or_else(|_| "nvim".to_string());

    let status = process::Command::new(&editor)
        .arg(&file_path)
        .status()
        .expect("failed to launch editor");

    process::exit(status.code().unwrap_or(1));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_titlecase_simple() {
        assert_eq!(titlecase("joins"), "Joins");
    }

    #[test]
    fn test_titlecase_hyphenated() {
        assert_eq!(titlecase("design-decisions"), "Design Decisions");
    }

    #[test]
    fn test_titlecase_underscored() {
        assert_eq!(titlecase("my_project"), "My Project");
    }

    #[test]
    fn test_titlecase_allcaps() {
        assert_eq!(titlecase("ALLCAPS"), "ALLCAPS");
    }

    #[test]
    fn test_resolve_daily_note() {
        let (path, header) = resolve_note(None);
        let today = Local::now().format("%Y-%m-%d").to_string();
        let parts: Vec<&str> = today.splitn(2, '-').collect();
        let expected_path = PathBuf::from("daily")
            .join(parts[0])
            .join(format!("{}.md", parts[1]));
        assert_eq!(path, expected_path);
        assert_eq!(header, format!("# {today}"));
    }

    #[test]
    fn test_resolve_simple_note() {
        let (path, header) = resolve_note(Some("foo"));
        assert_eq!(path, PathBuf::from("foo.md"));
        assert_eq!(header, "# Foo");
    }

    #[test]
    fn test_resolve_nested_note() {
        let (path, header) = resolve_note(Some("sql/joins"));
        assert_eq!(path, PathBuf::from("sql/joins.md"));
        assert_eq!(header, "# Joins");
    }

    #[test]
    fn test_resolve_hyphenated_name() {
        let (path, header) = resolve_note(Some("my-project/design-decisions"));
        assert_eq!(path, PathBuf::from("my-project/design-decisions.md"));
        assert_eq!(header, "# Design Decisions");
    }

    #[test]
    fn test_resolve_trailing_slash_uses_date() {
        let today = Local::now().format("%Y-%m-%d").to_string();
        let (path, header) = resolve_note(Some("my-dir/"));
        assert_eq!(path, PathBuf::from(format!("my-dir/{today}.md")));
        assert_eq!(header, format!("# {today}"));
    }

    #[test]
    fn test_resolve_nested_trailing_slash() {
        let today = Local::now().format("%Y-%m-%d").to_string();
        let (path, header) = resolve_note(Some("projects/myproject/"));
        assert_eq!(
            path,
            PathBuf::from(format!("projects/myproject/{today}.md"))
        );
        assert_eq!(header, format!("# {today}"));
    }

    #[test]
    fn test_daily_note_creates_dirs_and_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = open_note(tmp.path(), None);

        assert!(path.exists());
        assert!(path.starts_with(tmp.path().join("daily")));
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.starts_with("# 2"));
        assert!(content.ends_with("\n\n"));
    }

    #[test]
    fn test_nested_note_creates_dirs() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = open_note(tmp.path(), Some("sql/joins"));

        assert_eq!(path, tmp.path().join("sql/joins.md"));
        assert!(tmp.path().join("sql").is_dir());
        let content = fs::read_to_string(&path).unwrap();
        assert_eq!(content, "# Joins\n\n");
    }

    #[test]
    fn test_existing_file_not_overwritten() {
        let tmp = tempfile::TempDir::new().unwrap();

        open_note(tmp.path(), Some("foo"));
        fs::write(tmp.path().join("foo.md"), "# Foo\n\nMy notes here\n").unwrap();

        open_note(tmp.path(), Some("foo"));
        let content = fs::read_to_string(tmp.path().join("foo.md")).unwrap();
        assert_eq!(content, "# Foo\n\nMy notes here\n");
    }

    #[test]
    fn test_trailing_slash_creates_dir_and_dated_note() {
        let tmp = tempfile::TempDir::new().unwrap();
        let today = Local::now().format("%Y-%m-%d").to_string();
        let path = open_note(tmp.path(), Some("my-dir/"));

        assert_eq!(path, tmp.path().join(format!("my-dir/{today}.md")));
        assert!(tmp.path().join("my-dir").is_dir());
        let content = fs::read_to_string(&path).unwrap();
        assert_eq!(content, format!("# {today}\n\n"));
    }

    #[test]
    fn test_empty_file_gets_header() {
        let tmp = tempfile::TempDir::new().unwrap();
        let file = tmp.path().join("foo.md");
        fs::write(&file, "").unwrap();

        open_note(tmp.path(), Some("foo"));

        let content = fs::read_to_string(&file).unwrap();
        assert_eq!(content, "# Foo\n\n");
    }

    #[test]
    fn test_append_to_note() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = open_note(tmp.path(), Some("foo"));

        append_to_note(&path, "first line");
        append_to_note(&path, "second line");

        let content = fs::read_to_string(&path).unwrap();
        assert_eq!(content, "# Foo\n\nfirst line\nsecond line\n");
    }

    #[test]
    fn test_append_to_daily_note() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = open_note(tmp.path(), None);

        append_to_note(&path, "quick thought");

        let content = fs::read_to_string(&path).unwrap();
        assert!(content.ends_with("quick thought\n"));
    }

    #[test]
    fn test_append_creates_note_if_new() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = open_note(tmp.path(), Some("new-note"));

        append_to_note(&path, "first entry");

        let content = fs::read_to_string(&path).unwrap();
        assert_eq!(content, "# New Note\n\nfirst entry\n");
    }

    #[test]
    fn test_append_text_starting_with_hyphen() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = open_note(tmp.path(), Some("foo"));

        append_to_note(&path, "- my note");

        let content = fs::read_to_string(&path).unwrap();
        assert_eq!(content, "# Foo\n\n- my note\n");
    }

    #[test]
    fn test_cli_parses_hyphen_append() {
        let cli = Cli::parse_from(["kno", "-a", "- my note"]);
        assert_eq!(cli.append.as_deref(), Some("- my note"));
        assert!(cli.path.is_none());
    }

    #[test]
    fn test_cli_parses_hyphen_append_with_path() {
        let cli = Cli::parse_from(["kno", "sql/joins", "-a", "- todo item"]);
        assert_eq!(cli.path.as_deref(), Some("sql/joins"));
        assert_eq!(cli.append.as_deref(), Some("- todo item"));
    }

    fn setup_test_notes(tmp: &std::path::Path) {
        // Create a structure:
        // daily/2026/02-15.md
        // sql/joins.md
        // my-project/design-decisions.md
        // my-project/ideas.md
        fs::create_dir_all(tmp.join("daily/2026")).unwrap();
        fs::write(tmp.join("daily/2026/02-15.md"), "# 2026-02-15\n\n").unwrap();
        fs::create_dir_all(tmp.join("sql")).unwrap();
        fs::write(tmp.join("sql/joins.md"), "# Joins\n\n").unwrap();
        fs::create_dir_all(tmp.join("my-project")).unwrap();
        fs::write(tmp.join("my-project/design-decisions.md"), "# Design Decisions\n\n").unwrap();
        fs::write(tmp.join("my-project/ideas.md"), "# Ideas\n\n").unwrap();
    }

    #[test]
    fn test_list_dirs_only() {
        let tmp = tempfile::TempDir::new().unwrap();
        setup_test_notes(tmp.path());

        let output = list_notes(tmp.path(), None, false);
        assert_eq!(
            output,
            "\
.\n\
├── daily\n\
│   └── 2026\n\
├── my-project\n\
└── sql\n"
        );
    }

    #[test]
    fn test_list_with_notes() {
        let tmp = tempfile::TempDir::new().unwrap();
        setup_test_notes(tmp.path());

        let output = list_notes(tmp.path(), None, true);
        let expected = [
            ".",
            "├── daily",
            "│   └── 2026",
            "│       └── 02-15",
            "├── my-project",
            "│   ├── design-decisions",
            "│   └── ideas",
            "└── sql",
            "    └── joins",
            "",
        ]
        .join("\n");
        assert_eq!(output, expected);
    }

    #[test]
    fn test_list_subtree() {
        let tmp = tempfile::TempDir::new().unwrap();
        setup_test_notes(tmp.path());

        let output = list_notes(tmp.path(), Some("my-project"), false);
        // No subdirs in my-project, so just the label
        assert_eq!(output, "my-project\n");
    }

    #[test]
    fn test_list_subtree_with_notes() {
        let tmp = tempfile::TempDir::new().unwrap();
        setup_test_notes(tmp.path());

        let output = list_notes(tmp.path(), Some("my-project"), true);
        assert_eq!(
            output,
            "\
my-project\n\
├── design-decisions\n\
└── ideas\n"
        );
    }

    #[test]
    fn test_list_empty_dir() {
        let tmp = tempfile::TempDir::new().unwrap();

        let output = list_notes(tmp.path(), None, false);
        assert_eq!(output, ".\n");
    }

    #[test]
    fn test_cli_parses_list_subcommand() {
        let cli = Cli::parse_from(["kno", "list"]);
        assert!(matches!(
            cli.command,
            Some(Command::List { notes: false, .. })
        ));
    }

    #[test]
    fn test_cli_parses_list_with_notes_flag() {
        let cli = Cli::parse_from(["kno", "list", "-n"]);
        assert!(matches!(
            cli.command,
            Some(Command::List { notes: true, .. })
        ));
    }

    #[test]
    fn test_cli_parses_list_with_path() {
        let cli = Cli::parse_from(["kno", "list", "sql"]);
        match &cli.command {
            Some(Command::List { path, notes }) => {
                assert_eq!(path.as_deref(), Some("sql"));
                assert!(!notes);
            }
            _ => panic!("expected List command"),
        }
    }

    #[test]
    fn test_cli_parses_git_subcommand() {
        let cli = Cli::parse_from(["kno", "git", "status"]);
        match &cli.command {
            Some(Command::Git { args }) => {
                assert_eq!(args, &["status"]);
            }
            _ => panic!("expected Git command"),
        }
    }

    #[test]
    fn test_cli_parses_git_with_multiple_args() {
        let cli = Cli::parse_from(["kno", "git", "log", "--oneline"]);
        match &cli.command {
            Some(Command::Git { args }) => {
                assert_eq!(args, &["log", "--oneline"]);
            }
            _ => panic!("expected Git command"),
        }
    }

    #[test]
    fn test_cli_parses_git_no_args() {
        let cli = Cli::parse_from(["kno", "git"]);
        match &cli.command {
            Some(Command::Git { args }) => {
                assert!(args.is_empty());
            }
            _ => panic!("expected Git command"),
        }
    }

    #[test]
    fn test_git_init_creates_repo() {
        let tmp = tempfile::TempDir::new().unwrap();
        let notes_dir = tmp.path().join("notes");
        fs::create_dir_all(&notes_dir).unwrap();

        assert!(!notes_dir.join(".git").exists());

        let status = process::Command::new("git")
            .arg("-C")
            .arg(&notes_dir)
            .arg("init")
            .status()
            .unwrap();
        assert!(status.success());
        assert!(notes_dir.join(".git").exists());
    }

    #[test]
    fn test_list_hides_dot_dirs() {
        let tmp = tempfile::TempDir::new().unwrap();
        setup_test_notes(tmp.path());

        // Create a .git dir and a .templates dir
        fs::create_dir_all(tmp.path().join(".git")).unwrap();
        fs::create_dir_all(tmp.path().join(".templates")).unwrap();

        let output = list_notes(tmp.path(), None, false);
        assert!(!output.contains(".git"));
        assert!(!output.contains(".templates"));
        // But regular dirs still show
        assert!(output.contains("daily"));
        assert!(output.contains("sql"));
    }
}
