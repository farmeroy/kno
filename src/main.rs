use std::env;
use std::fs;
use std::path::PathBuf;
use std::process;

use chrono::Local;
use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
use clap_complete::engine::{ArgValueCompleter, PathCompleter};

const NOTES_DIR_NAME: &str = ".kno";

#[derive(Parser)]
#[command(about = "A simple notes CLI", args_conflicts_with_subcommands = true)]
struct Cli {
    /// Note path (e.g. sql/joins). Opens daily note if omitted.
    path: Option<String>,

    /// Print the resolved file path instead of opening the editor.
    /// Convenience for vim integration, e.g. nnoremap <leader>kn :execute 'e' trim(system('kno -p'))<CR>
    /// TODO: revisit — this resolves a specific note path rather than the notes dir,
    /// and has the side effect of creating the file. May want to rethink the semantics.
    #[arg(short, long)]
    print: bool,

    /// Text to append to the note. Appends and exits without opening editor.
    #[arg(short, long, allow_hyphen_values = true)]
    append: Option<String>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// List notes and directories
    List {
        /// Directory to list (lists all if omitted)
        path: Option<String>,

        /// Max depth to display (default: 1, 0 for unlimited)
        #[arg(short = 'L', long)]
        level: Option<usize>,
    },

    /// Initialize kno: create notes dir, git repo, and shell completions
    Init,

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

fn list_tree(dir: &std::path::Path, prefix: &str, max_depth: Option<usize>, depth: usize, output: &mut String) {
    if max_depth.is_some_and(|m| depth >= m) {
        return;
    }

    let mut entries: Vec<_> = match fs::read_dir(dir) {
        Ok(rd) => rd.filter_map(|e| e.ok()).collect(),
        Err(_) => return,
    };
    entries.sort_by_key(|e| e.file_name());

    let entries: Vec<_> = entries
        .into_iter()
        .filter(|e| {
            let name = e.file_name();
            let name_str = name.to_string_lossy();
            let ft = e.file_type().unwrap();
            if ft.is_dir() {
                !name_str.starts_with('.')
            } else {
                e.path().extension().is_some_and(|ext| ext == "md")
            }
        })
        .collect();

    for (i, entry) in entries.iter().enumerate() {
        let is_last = i == entries.len() - 1;
        let connector = if is_last { "└── " } else { "├── " };
        let name = entry.file_name();
        let display_name = if entry.file_type().unwrap().is_file() {
            name.to_string_lossy().to_string()
        } else {
            format!("{}/", name.to_string_lossy())
        };

        output.push_str(&format!("{prefix}{connector}{display_name}\n"));

        if entry.file_type().unwrap().is_dir() {
            let child_prefix = if is_last {
                format!("{prefix}    ")
            } else {
                format!("{prefix}│   ")
            };
            list_tree(&entry.path(), &child_prefix, max_depth, depth + 1, output);
        }
    }
}

fn list_notes(notes_dir: &std::path::Path, path: Option<&str>, max_depth: Option<usize>) -> String {
    let root = match path {
        Some(p) => notes_dir.join(p),
        None => notes_dir.to_path_buf(),
    };

    if !root.is_dir() {
        return format!("{} is not a directory\n", root.display());
    }

    let label = path.unwrap_or(".");
    let mut output = format!("{label}\n");
    list_tree(&root, "", max_depth, 0, &mut output);
    output
}

fn create_notes_dir(notes_dir: &std::path::Path) {
    if !notes_dir.exists() {
        fs::create_dir_all(notes_dir).expect("failed to create notes directory");
        println!("Created {}", notes_dir.display());
    } else {
        println!("{} already exists", notes_dir.display());
    }
}

fn init_git_repo(notes_dir: &std::path::Path) {
    if notes_dir.join(".git").exists() {
        println!("Git repo already initialized");
        return;
    }

    match process::Command::new("git")
        .arg("-C")
        .arg(notes_dir)
        .arg("init")
        .status()
    {
        Ok(s) if s.success() => println!("Initialized git repo"),
        Ok(_) => eprintln!("Warning: git init failed"),
        Err(e) => eprintln!("Warning: could not run git: {e}"),
    }
}

fn setup_shell_completions() {
    let home = env::var("HOME").expect("HOME not set");
    let zshrc = PathBuf::from(&home).join(".zshrc");
    let completion_line = "source <(COMPLETE=zsh kno)";

    let already_present = zshrc
        .exists()
        .then(|| fs::read_to_string(&zshrc).unwrap_or_default())
        .is_some_and(|content| content.contains(completion_line));

    if already_present {
        println!("Shell completions already configured");
        return;
    }

    let result = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&zshrc)
        .and_then(|mut file| {
            use std::io::Write;
            writeln!(file, "\n{completion_line}")
        });

    match result {
        Ok(()) => println!("Added shell completions to ~/.zshrc (restart your shell or `source ~/.zshrc`)"),
        Err(e) => eprintln!("Warning: could not update .zshrc: {e}"),
    }
}

fn run_init(notes_dir: &std::path::Path) {
    create_notes_dir(notes_dir);
    init_git_repo(notes_dir);
    setup_shell_completions();
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
    let home = env::var("HOME").expect("HOME not set");
    let notes_dir = PathBuf::from(&home).join(NOTES_DIR_NAME);

    let mut cmd = Cli::command();
    cmd = cmd.mut_arg("path", |a| {
        a.add(ArgValueCompleter::new(
            PathCompleter::any().current_dir(&notes_dir),
        ))
    });
    clap_complete::CompleteEnv::with_factory(|| cmd.clone()).complete();

    let cli = Cli::from_arg_matches_mut(&mut cmd.get_matches_from(env::args_os())).unwrap();

    match cli.command {
        Some(Command::Init) => {
            run_init(&notes_dir);
            return;
        }
        Some(Command::Git { ref args }) => {
            run_git(&notes_dir, args);
        }
        Some(Command::List { ref path, level }) => {
            let depth = Some(level.unwrap_or(1)).filter(|&l| l > 0);
            let output = list_notes(&notes_dir, path.as_deref(), depth);
            print!("{output}");
            return;
        }
        None => {}
    }

    let file_path = open_note(&notes_dir, cli.path.as_deref());

    if cli.print {
        println!("{}", file_path.display());
        return;
    }

    if let Some(text) = &cli.append {
        append_to_note(&file_path, text);
        return;
    }

    let editor = env::var("EDITOR").unwrap_or_else(|_| "nvim".to_string());

    let status = process::Command::new(&editor)
        .arg(&file_path)
        .current_dir(&notes_dir)
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
        fs::write(
            tmp.join("my-project/design-decisions.md"),
            "# Design Decisions\n\n",
        )
        .unwrap();
        fs::write(tmp.join("my-project/ideas.md"), "# Ideas\n\n").unwrap();
    }

    #[test]
    fn test_list_all() {
        let tmp = tempfile::TempDir::new().unwrap();
        setup_test_notes(tmp.path());

        let output = list_notes(tmp.path(), None, None);
        let expected = [
            ".",
            "├── daily/",
            "│   └── 2026/",
            "│       └── 02-15.md",
            "├── my-project/",
            "│   ├── design-decisions.md",
            "│   └── ideas.md",
            "└── sql/",
            "    └── joins.md",
            "",
        ]
        .join("\n");
        assert_eq!(output, expected);
    }

    #[test]
    fn test_list_with_depth() {
        let tmp = tempfile::TempDir::new().unwrap();
        setup_test_notes(tmp.path());

        let output = list_notes(tmp.path(), None, Some(1));
        assert_eq!(
            output,
            "\
.\n\
├── daily/\n\
├── my-project/\n\
└── sql/\n"
        );
    }

    #[test]
    fn test_list_subtree() {
        let tmp = tempfile::TempDir::new().unwrap();
        setup_test_notes(tmp.path());

        let output = list_notes(tmp.path(), Some("my-project"), None);
        assert_eq!(
            output,
            "\
my-project\n\
├── design-decisions.md\n\
└── ideas.md\n"
        );
    }

    #[test]
    fn test_list_empty_dir() {
        let tmp = tempfile::TempDir::new().unwrap();

        let output = list_notes(tmp.path(), None, None);
        assert_eq!(output, ".\n");
    }

    #[test]
    fn test_cli_parses_list_subcommand() {
        let cli = Cli::parse_from(["kno", "list"]);
        assert!(matches!(
            cli.command,
            Some(Command::List { level: None, .. })
        ));
    }

    #[test]
    fn test_cli_parses_list_with_level() {
        let cli = Cli::parse_from(["kno", "list", "-L", "2"]);
        assert!(matches!(
            cli.command,
            Some(Command::List { level: Some(2), .. })
        ));
    }

    #[test]
    fn test_cli_parses_list_with_path() {
        let cli = Cli::parse_from(["kno", "list", "sql"]);
        match &cli.command {
            Some(Command::List { path, level }) => {
                assert_eq!(path.as_deref(), Some("sql"));
                assert_eq!(*level, None);
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

        let output = list_notes(tmp.path(), None, None);
        assert!(!output.contains(".git"));
        assert!(!output.contains(".templates"));
        // But regular dirs still show
        assert!(output.contains("daily"));
        assert!(output.contains("sql"));
    }
}
