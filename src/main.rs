use std::env;
use std::fs;
use std::path::PathBuf;
use std::process;

use chrono::Local;
use clap::Parser;

#[derive(Parser)]
#[command(about = "A simple notes CLI")]
struct Cli {
    /// Note path (e.g. sql/joins). Opens daily note if omitted.
    path: Vec<String>,
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

fn resolve_note(args: &[String]) -> (PathBuf, String) {
    let today = Local::now().format("%Y-%m-%d").to_string();

    if args.is_empty() {
        // Default: daily directory with year grouping
        let parts: Vec<&str> = today.splitn(2, '-').collect();
        let year = parts[0];
        let rest = parts[1]; // MM-DD
        let path = PathBuf::from("daily").join(year).join(format!("{rest}.md"));
        let header = format!("# {today}");
        (path, header)
    } else {
        let note_path = args.join("/");
        if note_path.ends_with('/') {
            // Trailing slash: treat as directory, use today's date as filename
            let path = PathBuf::from(&note_path).join(format!("{today}.md"));
            let header = format!("# {today}");
            (path, header)
        } else {
            // Explicit note name
            let path = PathBuf::from(format!("{note_path}.md"));
            let stem = path
                .file_stem()
                .unwrap()
                .to_string_lossy();
            let header = format!("# {}", titlecase(&stem));
            (path, header)
        }
    }
}

fn open_note(notes_dir: &std::path::Path, args: &[String]) -> PathBuf {
    let (relative_path, header) = resolve_note(args);
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

fn main() {
    let cli = Cli::parse();

    let home = env::var("HOME").expect("HOME not set");
    let notes_dir = PathBuf::from(&home).join(".notes");

    let file_path = open_note(&notes_dir, &cli.path);

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
        let (path, header) = resolve_note(&[]);
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
        let args = vec!["foo".to_string()];
        let (path, header) = resolve_note(&args);
        assert_eq!(path, PathBuf::from("foo.md"));
        assert_eq!(header, "# Foo");
    }

    #[test]
    fn test_resolve_nested_note() {
        let args = vec!["sql/joins".to_string()];
        let (path, header) = resolve_note(&args);
        assert_eq!(path, PathBuf::from("sql/joins.md"));
        assert_eq!(header, "# Joins");
    }

    #[test]
    fn test_resolve_multi_args_joined() {
        let args = vec!["sql".to_string(), "joins".to_string()];
        let (path, header) = resolve_note(&args);
        assert_eq!(path, PathBuf::from("sql/joins.md"));
        assert_eq!(header, "# Joins");
    }

    #[test]
    fn test_resolve_hyphenated_name() {
        let args = vec!["my-project/design-decisions".to_string()];
        let (path, header) = resolve_note(&args);
        assert_eq!(path, PathBuf::from("my-project/design-decisions.md"));
        assert_eq!(header, "# Design Decisions");
    }

    #[test]
    fn test_daily_note_creates_dirs_and_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = open_note(tmp.path(), &[]);

        assert!(path.exists());
        assert!(path.starts_with(tmp.path().join("daily")));
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.starts_with("# 2"));
        assert!(content.ends_with("\n\n"));
    }

    #[test]
    fn test_nested_note_creates_dirs() {
        let tmp = tempfile::TempDir::new().unwrap();
        let args = vec!["sql/joins".to_string()];
        let path = open_note(tmp.path(), &args);

        assert_eq!(path, tmp.path().join("sql/joins.md"));
        assert!(tmp.path().join("sql").is_dir());
        let content = fs::read_to_string(&path).unwrap();
        assert_eq!(content, "# Joins\n\n");
    }

    #[test]
    fn test_existing_file_not_overwritten() {
        let tmp = tempfile::TempDir::new().unwrap();
        let args = vec!["foo".to_string()];

        open_note(tmp.path(), &args);

        fs::write(tmp.path().join("foo.md"), "# Foo\n\nMy notes here\n").unwrap();

        open_note(tmp.path(), &args);
        let content = fs::read_to_string(tmp.path().join("foo.md")).unwrap();
        assert_eq!(content, "# Foo\n\nMy notes here\n");
    }

    #[test]
    fn test_resolve_trailing_slash_uses_date() {
        let today = Local::now().format("%Y-%m-%d").to_string();
        let args = vec!["my-dir/".to_string()];
        let (path, header) = resolve_note(&args);
        assert_eq!(path, PathBuf::from(format!("my-dir/{today}.md")));
        assert_eq!(header, format!("# {today}"));
    }

    #[test]
    fn test_resolve_nested_trailing_slash() {
        let today = Local::now().format("%Y-%m-%d").to_string();
        let args = vec!["projects/myproject/".to_string()];
        let (path, header) = resolve_note(&args);
        assert_eq!(
            path,
            PathBuf::from(format!("projects/myproject/{today}.md"))
        );
        assert_eq!(header, format!("# {today}"));
    }

    #[test]
    fn test_trailing_slash_creates_dir_and_dated_note() {
        let tmp = tempfile::TempDir::new().unwrap();
        let today = Local::now().format("%Y-%m-%d").to_string();
        let args = vec!["my-dir/".to_string()];
        let path = open_note(tmp.path(), &args);

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

        let args = vec!["foo".to_string()];
        open_note(tmp.path(), &args);

        let content = fs::read_to_string(&file).unwrap();
        assert_eq!(content, "# Foo\n\n");
    }
}
