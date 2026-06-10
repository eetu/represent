//! Flat-file project store: `data_dir/<project>/<file>.md`. No DB — the
//! filesystem is the source of truth, which keeps the data trivially
//! restic-backed, rsync-able, and editable out-of-band (files are generated
//! elsewhere and copied in). Every name crosses a trust boundary (URL path →
//! filesystem path), so both segments go through the same strict allowlist
//! before any path is built.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::{AppError, AppResult};

/// Max accepted name length (project or file). Generous for human titles,
/// small enough to keep paths well under any filesystem limit.
const MAX_NAME_LEN: usize = 128;

#[derive(Debug, Serialize)]
pub struct ProjectInfo {
    pub name: String,
    pub file_count: usize,
    /// Most recent mtime among the project's markdown files (RFC 3339), or
    /// null for an empty project.
    pub updated_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct FileInfo {
    pub name: String,
    pub size: u64,
    pub modified: Option<String>,
}

/// A name is a single safe path segment: no separators, no traversal, no
/// hidden files, no leading/trailing whitespace. Allowlist, not blocklist.
///
/// Non-ASCII is allowed wholesale (minus control chars): ö/ä/å are perfectly
/// valid file names, and macOS stores them NFD-decomposed (`ö` = `o` +
/// combining mark), so a letters-only Unicode check would reject what the
/// filesystem itself hands back. Everything path-dangerous (`/`, `\`, NUL,
/// `..`) is ASCII and stays excluded.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_NAME_LEN
        && !name.starts_with(['.', ' '])
        && !name.ends_with([' ', '.'])
        && name.chars().all(|c| {
            c.is_ascii_alphanumeric()
                || matches!(c, '.' | '_' | '-' | ' ')
                || (!c.is_ascii() && !c.is_control())
        })
}

/// Markdown file names additionally require the `.md` extension so the store
/// can never be used to park arbitrary file types.
pub fn valid_md_name(name: &str) -> bool {
    valid_name(name) && name.len() > 3 && name.ends_with(".md")
}

fn project_dir(data_dir: &Path, project: &str) -> AppResult<PathBuf> {
    if !valid_name(project) {
        return Err(AppError::BadRequest("invalid project name".into()));
    }
    Ok(data_dir.join(project))
}

fn file_path(data_dir: &Path, project: &str, file: &str) -> AppResult<PathBuf> {
    let dir = project_dir(data_dir, project)?;
    if !valid_md_name(file) {
        return Err(AppError::BadRequest(
            "invalid file name (must be *.md)".into(),
        ));
    }
    Ok(dir.join(file))
}

fn mtime_rfc3339(meta: &std::fs::Metadata) -> Option<String> {
    let t: chrono::DateTime<chrono::Utc> = meta.modified().ok()?.into();
    Some(t.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
}

pub async fn list_projects(data_dir: &Path) -> AppResult<Vec<ProjectInfo>> {
    let mut out = Vec::new();
    let mut entries = tokio::fs::read_dir(data_dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name().to_string_lossy().to_string();
        if !valid_name(&name) || !entry.file_type().await?.is_dir() {
            continue;
        }
        let files = list_files(data_dir, &name).await?;
        out.push(ProjectInfo {
            updated_at: files.iter().filter_map(|f| f.modified.clone()).max(),
            file_count: files.len(),
            name,
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

pub async fn create_project(data_dir: &Path, project: &str) -> AppResult<()> {
    let dir = project_dir(data_dir, project)?;
    if dir.exists() {
        return Err(AppError::Conflict("project already exists".into()));
    }
    tokio::fs::create_dir(&dir).await?;
    Ok(())
}

pub async fn delete_project(data_dir: &Path, project: &str) -> AppResult<()> {
    let dir = project_dir(data_dir, project)?;
    if !dir.is_dir() {
        return Err(AppError::NotFound);
    }
    tokio::fs::remove_dir_all(&dir).await?;
    Ok(())
}

/// Per-project ordering sidecar: a JSON array of file names. Hidden (dot)
/// name, so it can never collide with the API's name allowlist, never lists,
/// and never ships in a bundle. Stale entries (deleted files) are simply
/// ignored on read; files not in it sort alphabetically after the ordered
/// ones — a fresh upload lands at the end.
const ORDER_FILE: &str = ".order";

async fn read_order(dir: &Path) -> Vec<String> {
    match tokio::fs::read(dir.join(ORDER_FILE)).await {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

/// Markdown files of a project in demo order: the `.order` sidecar decides;
/// anything unlisted follows alphabetically. File names are never touched —
/// ordering is metadata, not a naming convention.
pub async fn list_files(data_dir: &Path, project: &str) -> AppResult<Vec<FileInfo>> {
    let dir = project_dir(data_dir, project)?;
    if !dir.is_dir() {
        return Err(AppError::NotFound);
    }
    let mut out = Vec::new();
    let mut entries = tokio::fs::read_dir(&dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name().to_string_lossy().to_string();
        if !valid_md_name(&name) || !entry.file_type().await?.is_file() {
            continue;
        }
        let meta = entry.metadata().await?;
        out.push(FileInfo {
            name,
            size: meta.len(),
            modified: mtime_rfc3339(&meta),
        });
    }
    let order = read_order(&dir).await;
    let pos = |f: &FileInfo| order.iter().position(|n| n == &f.name).unwrap_or(usize::MAX);
    out.sort_by(|a, b| pos(a).cmp(&pos(b)).then_with(|| a.name.cmp(&b.name)));
    Ok(out)
}

/// Persist a new demo order. File names are kept exactly as uploaded — the
/// sequence goes into the `.order` sidecar, nothing on disk is renamed.
/// `order` must be a permutation of the project's current files.
pub async fn reorder(data_dir: &Path, project: &str, order: &[String]) -> AppResult<Vec<FileInfo>> {
    let dir = project_dir(data_dir, project)?;
    let current = list_files(data_dir, project).await?;

    let mut want: Vec<&str> = order.iter().map(String::as_str).collect();
    want.sort_unstable();
    let mut have: Vec<&str> = current.iter().map(|f| f.name.as_str()).collect();
    have.sort_unstable();
    if want != have {
        return Err(AppError::BadRequest(
            "order must list exactly the project's files".into(),
        ));
    }

    let json = serde_json::to_vec(order).map_err(anyhow::Error::from)?;
    tokio::fs::write(dir.join(ORDER_FILE), json).await?;
    list_files(data_dir, project).await
}

pub async fn read_file(data_dir: &Path, project: &str, file: &str) -> AppResult<String> {
    let path = file_path(data_dir, project, file)?;
    Ok(tokio::fs::read_to_string(&path).await?)
}

/// Upsert. The project must already exist — a typo'd project segment must not
/// silently create a directory.
pub async fn write_file(
    data_dir: &Path,
    project: &str,
    file: &str,
    content: &str,
) -> AppResult<()> {
    let path = file_path(data_dir, project, file)?;
    if !project_dir(data_dir, project)?.is_dir() {
        return Err(AppError::NotFound);
    }
    tokio::fs::write(&path, content).await?;
    Ok(())
}

/// Rename one file, keeping its slot in the demo order. Refuses to overwrite.
pub async fn rename_file(data_dir: &Path, project: &str, from: &str, to: &str) -> AppResult<()> {
    let from_path = file_path(data_dir, project, from)?;
    let to_path = file_path(data_dir, project, to)?;
    if !from_path.is_file() {
        return Err(AppError::NotFound);
    }
    if to_path.exists() {
        return Err(AppError::Conflict("a file with that name exists".into()));
    }
    tokio::fs::rename(&from_path, &to_path).await?;

    let dir = project_dir(data_dir, project)?;
    let mut order = read_order(&dir).await;
    if let Some(entry) = order.iter_mut().find(|n| n.as_str() == from) {
        *entry = to.to_string();
        let json = serde_json::to_vec(&order).map_err(anyhow::Error::from)?;
        tokio::fs::write(dir.join(ORDER_FILE), json).await?;
    }
    Ok(())
}

pub async fn delete_file(data_dir: &Path, project: &str, file: &str) -> AppResult<()> {
    let path = file_path(data_dir, project, file)?;
    if !path.is_file() {
        return Err(AppError::NotFound);
    }
    tokio::fs::remove_file(&path).await?;
    Ok(())
}

/// Zip the project's markdown files in-memory (scripts are kilobytes, not
/// media). Runs on the blocking pool — zip's API is sync.
pub async fn bundle(data_dir: &Path, project: &str) -> AppResult<Vec<u8>> {
    let dir = project_dir(data_dir, project)?;
    if !dir.is_dir() {
        return Err(AppError::NotFound);
    }
    let files = list_files(data_dir, project).await?;
    let mut contents = Vec::with_capacity(files.len());
    for f in &files {
        contents.push((f.name.clone(), read_file(data_dir, project, &f.name).await?));
    }
    let zipped = tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<u8>> {
        use std::io::Write;
        let mut buf = std::io::Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(&mut buf);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (name, content) in contents {
            zip.start_file(name, opts)?;
            zip.write_all(content.as_bytes())?;
        }
        zip.finish()?;
        Ok(buf.into_inner())
    })
    .await
    .map_err(|e| AppError::Internal(e.into()))??;
    Ok(zipped)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_names() {
        assert!(valid_name("demo"));
        assert!(valid_name("Demo 2026-06 v1.2"));
        assert!(valid_md_name("01-intro.md"));
        // Unicode, both normalization forms (macOS hands back NFD).
        assert!(valid_md_name("yhteenveto äöå.md"));
        assert!(valid_md_name("yhteenveto a\u{308}o\u{308}a\u{30a}.md"));
    }

    #[test]
    fn rejects_traversal_and_hidden() {
        assert!(!valid_name(""));
        assert!(!valid_name("../etc"));
        assert!(!valid_name("a/b"));
        assert!(!valid_name("a\\b"));
        assert!(!valid_name(".hidden"));
        assert!(!valid_name(" lead"));
        assert!(!valid_name("trail "));
        assert!(!valid_name("trail."));
        assert!(!valid_name(&"x".repeat(129)));
        assert!(!valid_md_name("notes.txt"));
        assert!(!valid_md_name(".md"));
        assert!(!valid_md_name("..md"));
    }

    #[tokio::test]
    async fn crud_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let data = tmp.path();

        create_project(data, "demo").await.unwrap();
        assert!(matches!(
            create_project(data, "demo").await,
            Err(AppError::Conflict(_))
        ));

        write_file(data, "demo", "01-intro.md", "# hi").await.unwrap();
        write_file(data, "demo", "02-next.md", "# next").await.unwrap();
        assert_eq!(read_file(data, "demo", "01-intro.md").await.unwrap(), "# hi");

        let files = list_files(data, "demo").await.unwrap();
        assert_eq!(
            files.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(),
            ["01-intro.md", "02-next.md"]
        );

        let projects = list_projects(data).await.unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].file_count, 2);
        assert!(projects[0].updated_at.is_some());

        let zip_bytes = bundle(data, "demo").await.unwrap();
        assert_eq!(&zip_bytes[..2], b"PK");

        delete_file(data, "demo", "01-intro.md").await.unwrap();
        assert!(matches!(
            read_file(data, "demo", "01-intro.md").await,
            Err(AppError::NotFound)
        ));
        delete_project(data, "demo").await.unwrap();
        assert!(matches!(
            list_files(data, "demo").await,
            Err(AppError::NotFound)
        ));
    }

    #[tokio::test]
    async fn reorder_is_metadata_names_untouched() {
        let tmp = tempfile::tempdir().unwrap();
        let data = tmp.path();
        create_project(data, "demo").await.unwrap();
        write_file(data, "demo", "alpha.md", "a").await.unwrap();
        write_file(data, "demo", "beta.md", "b").await.unwrap();
        write_file(data, "demo", "gamma.md", "c").await.unwrap();

        let order = vec!["gamma.md".into(), "alpha.md".into(), "beta.md".into()];
        let files = reorder(data, "demo", &order).await.unwrap();
        assert_eq!(
            files.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(),
            ["gamma.md", "alpha.md", "beta.md"]
        );
        // Content stayed with its (unrenamed) file.
        assert_eq!(read_file(data, "demo", "gamma.md").await.unwrap(), "c");

        // A fresh upload lands at the end, after the ordered files.
        write_file(data, "demo", "delta.md", "d").await.unwrap();
        let files = list_files(data, "demo").await.unwrap();
        assert_eq!(
            files.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(),
            ["gamma.md", "alpha.md", "beta.md", "delta.md"]
        );

        // Rename keeps the slot in the order.
        rename_file(data, "demo", "alpha.md", "omega.md").await.unwrap();
        let files = list_files(data, "demo").await.unwrap();
        assert_eq!(
            files.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(),
            ["gamma.md", "omega.md", "beta.md", "delta.md"]
        );
        // Renaming onto an existing file refuses.
        assert!(matches!(
            rename_file(data, "demo", "omega.md", "beta.md").await,
            Err(AppError::Conflict(_))
        ));

        // Deleting an ordered file leaves a stale .order entry — harmless.
        delete_file(data, "demo", "omega.md").await.unwrap();
        let files = list_files(data, "demo").await.unwrap();
        assert_eq!(
            files.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(),
            ["gamma.md", "beta.md", "delta.md"]
        );

        // Not a permutation → rejected, nothing changed.
        assert!(matches!(
            reorder(data, "demo", &["gamma.md".into()]).await,
            Err(AppError::BadRequest(_))
        ));
    }

    #[tokio::test]
    async fn write_to_missing_project_is_404_not_creation() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(matches!(
            write_file(tmp.path(), "nope", "a.md", "x").await,
            Err(AppError::NotFound)
        ));
        assert!(!tmp.path().join("nope").exists());
    }

    #[tokio::test]
    async fn invalid_names_rejected_before_fs() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(matches!(
            read_file(tmp.path(), "..", "a.md").await,
            Err(AppError::BadRequest(_))
        ));
        assert!(matches!(
            read_file(tmp.path(), "demo", "../../etc/passwd").await,
            Err(AppError::BadRequest(_))
        ));
    }
}
