//! SQLite-backed project store: `profile → projects → documents`, all keyed
//! by the authenticated profile id (multi-user). Names still pass the strict
//! allowlist — they end up in zip entries, Content-Disposition headers and
//! URLs even though they no longer touch the filesystem.

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use crate::db::Db;
use crate::error::{is_unique_violation, AppError, AppResult};

/// Max accepted name length (project or file).
const MAX_NAME_LEN: usize = 128;

#[derive(Debug, Serialize)]
pub struct ProjectInfo {
    pub name: String,
    pub file_count: i64,
    /// Most recent modification among the project's documents (RFC 3339),
    /// or null for an empty project.
    pub updated_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct FileInfo {
    pub name: String,
    pub size: i64,
    pub modified: Option<String>,
}

/// A name is a single safe segment: no separators, no leading dot/space, no
/// trailing dot/space, no control chars. Non-ASCII is allowed wholesale
/// (ö/ä/å are valid; macOS-NFD forms too) — everything header/zip-dangerous
/// is ASCII and stays excluded.
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

/// Document names additionally require the `.md` extension.
pub fn valid_md_name(name: &str) -> bool {
    valid_name(name) && name.len() > 3 && name.ends_with(".md")
}

fn check_project_name(name: &str) -> AppResult<()> {
    if valid_name(name) {
        Ok(())
    } else {
        Err(AppError::BadRequest("invalid project name".into()))
    }
}

fn check_md_name(name: &str) -> AppResult<()> {
    if valid_md_name(name) {
        Ok(())
    } else {
        Err(AppError::BadRequest(
            "invalid file name (must be *.md)".into(),
        ))
    }
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// Map an OIDC/forward-auth identity to a profile id: sub match → email
/// match (backfilling the sub for rows created via the other auth path) →
/// create. The email is the human-stable key across auth modes.
pub async fn resolve_profile(db: &Db, sub: &str, email: &str) -> AppResult<i64> {
    let sub = sub.to_string();
    let email = email.to_string();
    let id = db
        .with(move |c| {
            if let Some(id) = c
                .query_row(
                    "SELECT id FROM profile WHERE user_sub = ?1",
                    [&sub],
                    |r| r.get::<_, i64>(0),
                )
                .optional()?
            {
                return Ok(id);
            }
            if let Some(id) = c
                .query_row("SELECT id FROM profile WHERE email = ?1", [&email], |r| {
                    r.get::<_, i64>(0)
                })
                .optional()?
            {
                c.execute(
                    "UPDATE profile SET user_sub = ?1 WHERE id = ?2",
                    params![sub, id],
                )?;
                return Ok(id);
            }
            c.execute(
                "INSERT INTO profile (user_sub, email, created_at) VALUES (?1, ?2, ?3)",
                params![sub, email, now()],
            )?;
            Ok(c.last_insert_rowid())
        })
        .await?;
    Ok(id)
}

fn project_id(c: &Connection, profile_id: i64, name: &str) -> rusqlite::Result<Option<i64>> {
    c.query_row(
        "SELECT id FROM projects WHERE profile_id = ?1 AND name = ?2",
        params![profile_id, name],
        |r| r.get(0),
    )
    .optional()
}

pub async fn list_projects(db: &Db, profile_id: i64) -> AppResult<Vec<ProjectInfo>> {
    let rows = db
        .with(move |c| {
            let mut stmt = c.prepare(
                "SELECT p.name, COUNT(d.id), MAX(d.modified_at)
                 FROM projects p LEFT JOIN documents d ON d.project_id = p.id
                 WHERE p.profile_id = ?1
                 GROUP BY p.id ORDER BY p.name",
            )?;
            let rows = stmt
                .query_map([profile_id], |r| {
                    Ok(ProjectInfo {
                        name: r.get(0)?,
                        file_count: r.get(1)?,
                        updated_at: r.get(2)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
        .await?;
    Ok(rows)
}

pub async fn create_project(db: &Db, profile_id: i64, name: &str) -> AppResult<()> {
    check_project_name(name)?;
    let name = name.to_string();
    db.with(move |c| {
        c.execute(
            "INSERT INTO projects (profile_id, name, created_at) VALUES (?1, ?2, ?3)",
            params![profile_id, name, now()],
        )
        .map(|_| ())
    })
    .await
    .map_err(|e| {
        if is_unique_violation(&e) {
            AppError::Conflict("project already exists".into())
        } else {
            e.into()
        }
    })
}

pub async fn delete_project(db: &Db, profile_id: i64, name: &str) -> AppResult<()> {
    check_project_name(name)?;
    let name = name.to_string();
    let n = db
        .with(move |c| {
            c.execute(
                "DELETE FROM projects WHERE profile_id = ?1 AND name = ?2",
                params![profile_id, name],
            )
        })
        .await?;
    if n == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}

/// Documents of a project in demo order (`position`, then name for ties —
/// fresh uploads get max+1 so they land at the end).
pub async fn list_files(db: &Db, profile_id: i64, project: &str) -> AppResult<Vec<FileInfo>> {
    check_project_name(project)?;
    let project = project.to_string();
    let rows = db
        .with(move |c| {
            let Some(pid) = project_id(c, profile_id, &project)? else {
                return Ok(None);
            };
            let mut stmt = c.prepare(
                "SELECT name, LENGTH(CAST(content AS BLOB)), modified_at
                 FROM documents WHERE project_id = ?1 ORDER BY position, name",
            )?;
            let rows = stmt
                .query_map([pid], |r| {
                    Ok(FileInfo {
                        name: r.get(0)?,
                        size: r.get(1)?,
                        modified: r.get(2)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(Some(rows))
        })
        .await?;
    rows.ok_or(AppError::NotFound)
}

pub async fn read_file(db: &Db, profile_id: i64, project: &str, file: &str) -> AppResult<String> {
    check_project_name(project)?;
    check_md_name(file)?;
    let (project, file) = (project.to_string(), file.to_string());
    let content = db
        .with(move |c| {
            c.query_row(
                "SELECT d.content FROM documents d
                 JOIN projects p ON p.id = d.project_id
                 WHERE p.profile_id = ?1 AND p.name = ?2 AND d.name = ?3",
                params![profile_id, project, file],
                |r| r.get::<_, String>(0),
            )
            .optional()
        })
        .await?;
    content.ok_or(AppError::NotFound)
}

/// Upsert. The project must already exist — a typo'd project segment must
/// not silently create one.
pub async fn write_file(
    db: &Db,
    profile_id: i64,
    project: &str,
    file: &str,
    content: &str,
) -> AppResult<()> {
    check_project_name(project)?;
    check_md_name(file)?;
    let (project, file, content) = (project.to_string(), file.to_string(), content.to_string());
    let found = db
        .with(move |c| {
            let Some(pid) = project_id(c, profile_id, &project)? else {
                return Ok(false);
            };
            c.execute(
                "INSERT INTO documents (project_id, name, content, position, modified_at)
                 VALUES (?1, ?2, ?3,
                         (SELECT COALESCE(MAX(position) + 1, 0) FROM documents WHERE project_id = ?1),
                         ?4)
                 ON CONFLICT(project_id, name)
                 DO UPDATE SET content = excluded.content, modified_at = excluded.modified_at",
                params![pid, file, content, now()],
            )?;
            Ok(true)
        })
        .await?;
    if !found {
        return Err(AppError::NotFound);
    }
    Ok(())
}

/// Rename one document, keeping its demo-order position. 409 if taken.
pub async fn rename_file(
    db: &Db,
    profile_id: i64,
    project: &str,
    from: &str,
    to: &str,
) -> AppResult<()> {
    check_project_name(project)?;
    check_md_name(from)?;
    check_md_name(to)?;
    let (project, from, to) = (project.to_string(), from.to_string(), to.to_string());
    let n = db
        .with(move |c| {
            let Some(pid) = project_id(c, profile_id, &project)? else {
                return Ok(0);
            };
            c.execute(
                "UPDATE documents SET name = ?1 WHERE project_id = ?2 AND name = ?3",
                params![to, pid, from],
            )
        })
        .await
        .map_err(|e| {
            if is_unique_violation(&e) {
                AppError::Conflict("a file with that name exists".into())
            } else {
                AppError::from(e)
            }
        })?;
    if n == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}

pub async fn delete_file(db: &Db, profile_id: i64, project: &str, file: &str) -> AppResult<()> {
    check_project_name(project)?;
    check_md_name(file)?;
    let (project, file) = (project.to_string(), file.to_string());
    let n = db
        .with(move |c| {
            c.execute(
                "DELETE FROM documents WHERE name = ?3 AND project_id IN
                 (SELECT id FROM projects WHERE profile_id = ?1 AND name = ?2)",
                params![profile_id, project, file],
            )
        })
        .await?;
    if n == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}

/// Persist a new demo order — `order` must be a permutation of the project's
/// documents; positions are rewritten in one transaction.
pub async fn reorder(
    db: &Db,
    profile_id: i64,
    project: &str,
    order: &[String],
) -> AppResult<Vec<FileInfo>> {
    check_project_name(project)?;
    let project_name = project.to_string();
    let order_owned: Vec<String> = order.to_vec();
    let ok = db
        .with(move |c| {
            let Some(pid) = project_id(c, profile_id, &project_name)? else {
                return Ok(None);
            };
            let mut stmt =
                c.prepare("SELECT name FROM documents WHERE project_id = ?1 ORDER BY name")?;
            let mut have = stmt
                .query_map([pid], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            let mut want = order_owned.clone();
            have.sort_unstable();
            want.sort_unstable();
            if want != have {
                return Ok(Some(false));
            }
            // Single writer (mutex) — no BEGIN needed for atomicity against
            // other requests, but wrap anyway so a failure rolls back whole.
            let tx = c.unchecked_transaction()?;
            {
                let mut upd = tx.prepare(
                    "UPDATE documents SET position = ?1 WHERE project_id = ?2 AND name = ?3",
                )?;
                for (i, name) in order_owned.iter().enumerate() {
                    upd.execute(params![i as i64, pid, name])?;
                }
            }
            tx.commit()?;
            Ok(Some(true))
        })
        .await?;
    match ok {
        None => Err(AppError::NotFound),
        Some(false) => Err(AppError::BadRequest(
            "order must list exactly the project's files".into(),
        )),
        Some(true) => list_files(db, profile_id, project).await,
    }
}

/// Zip the project's documents in demo order (scripts are kilobytes). A
/// `.order` entry (JSON array of names) carries the sequence, so a bundle
/// re-imports — or moves to another instance — with its demo order intact
/// (`import_legacy` reads the same format).
pub async fn bundle(db: &Db, profile_id: i64, project: &str) -> AppResult<Vec<u8>> {
    check_project_name(project)?;
    let project = project.to_string();
    let contents = db
        .with(move |c| {
            let Some(pid) = project_id(c, profile_id, &project)? else {
                return Ok(None);
            };
            let mut stmt = c.prepare(
                "SELECT name, content FROM documents WHERE project_id = ?1 ORDER BY position, name",
            )?;
            let rows = stmt
                .query_map([pid], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(Some(rows))
        })
        .await?;
    let contents = contents.ok_or(AppError::NotFound)?;
    let zipped = tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<u8>> {
        use std::io::Write;
        let mut buf = std::io::Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(&mut buf);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        let order: Vec<&str> = contents.iter().map(|(n, _)| n.as_str()).collect();
        zip.start_file(".order", opts)?;
        zip.write_all(&serde_json::to_vec(&order)?)?;
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

/// One-shot boot import of the legacy flat-file layout
/// (`<dir>/<project>/*.md` + optional `.order` sidecar) into a profile.
/// Projects that already exist for the profile are skipped, so this is safe
/// to leave configured.
pub async fn import_legacy(db: &Db, dir: &std::path::Path, email: &str) -> AppResult<usize> {
    let profile_id = resolve_profile(db, email, email).await?;
    let mut imported = 0;
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let project = entry.file_name().to_string_lossy().to_string();
        if !entry.file_type()?.is_dir() || !valid_name(&project) {
            continue;
        }
        match create_project(db, profile_id, &project).await {
            Ok(()) => {}
            Err(AppError::Conflict(_)) => continue,
            Err(e) => return Err(e),
        }
        let order: Vec<String> = std::fs::read(entry.path().join(".order"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        let mut names: Vec<String> = std::fs::read_dir(entry.path())?
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| valid_md_name(n))
            .collect();
        let pos = |n: &String| order.iter().position(|o| o == n).unwrap_or(usize::MAX);
        names.sort_by(|a, b| pos(a).cmp(&pos(b)).then_with(|| a.cmp(b)));
        for name in names {
            let content = std::fs::read_to_string(entry.path().join(&name))?;
            write_file(db, profile_id, &project, &name, &content).await?;
        }
        imported += 1;
    }
    Ok(imported)
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_db() -> (Db, i64) {
        let db = Db::open_in_memory().unwrap();
        let pid = resolve_profile(&db, "tester", "tester@example.com")
            .await
            .unwrap();
        (db, pid)
    }

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
    fn rejects_dangerous_names() {
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
    }

    #[tokio::test]
    async fn crud_roundtrip() {
        let (db, pid) = test_db().await;

        create_project(&db, pid, "demo").await.unwrap();
        assert!(matches!(
            create_project(&db, pid, "demo").await,
            Err(AppError::Conflict(_))
        ));

        write_file(&db, pid, "demo", "intro.md", "# hi").await.unwrap();
        write_file(&db, pid, "demo", "next.md", "# next").await.unwrap();
        assert_eq!(read_file(&db, pid, "demo", "intro.md").await.unwrap(), "# hi");

        let files = list_files(&db, pid, "demo").await.unwrap();
        assert_eq!(
            files.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(),
            ["intro.md", "next.md"]
        );
        assert_eq!(files[0].size, 4);

        let projects = list_projects(&db, pid).await.unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].file_count, 2);
        assert!(projects[0].updated_at.is_some());

        let zip_bytes = bundle(&db, pid, "demo").await.unwrap();
        assert_eq!(&zip_bytes[..2], b"PK");
        // The .order entry carries the demo sequence for round-tripping.
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip_bytes)).unwrap();
        let mut order = String::new();
        std::io::Read::read_to_string(&mut archive.by_name(".order").unwrap(), &mut order)
            .unwrap();
        assert_eq!(order, r#"["intro.md","next.md"]"#);

        delete_file(&db, pid, "demo", "intro.md").await.unwrap();
        assert!(matches!(
            read_file(&db, pid, "demo", "intro.md").await,
            Err(AppError::NotFound)
        ));
        delete_project(&db, pid, "demo").await.unwrap();
        assert!(matches!(
            list_files(&db, pid, "demo").await,
            Err(AppError::NotFound)
        ));
    }

    #[tokio::test]
    async fn reorder_and_rename_keep_positions() {
        let (db, pid) = test_db().await;
        create_project(&db, pid, "demo").await.unwrap();
        for n in ["alpha.md", "beta.md", "gamma.md"] {
            write_file(&db, pid, "demo", n, "x").await.unwrap();
        }

        let order = vec!["gamma.md".into(), "alpha.md".into(), "beta.md".into()];
        let files = reorder(&db, pid, "demo", &order).await.unwrap();
        assert_eq!(
            files.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(),
            ["gamma.md", "alpha.md", "beta.md"]
        );

        // Fresh upload lands at the end.
        write_file(&db, pid, "demo", "delta.md", "d").await.unwrap();
        let files = list_files(&db, pid, "demo").await.unwrap();
        assert_eq!(files.last().unwrap().name, "delta.md");

        // Rename keeps the slot; clash is a 409.
        rename_file(&db, pid, "demo", "alpha.md", "omega.md").await.unwrap();
        let files = list_files(&db, pid, "demo").await.unwrap();
        assert_eq!(files[1].name, "omega.md");
        assert!(matches!(
            rename_file(&db, pid, "demo", "omega.md", "beta.md").await,
            Err(AppError::Conflict(_))
        ));

        // Not a permutation → rejected.
        assert!(matches!(
            reorder(&db, pid, "demo", &["gamma.md".into()]).await,
            Err(AppError::BadRequest(_))
        ));
    }

    #[tokio::test]
    async fn profiles_are_isolated() {
        let (db, alice) = test_db().await;
        let bob = resolve_profile(&db, "bob", "bob@example.com").await.unwrap();
        assert_ne!(alice, bob);

        create_project(&db, alice, "demo").await.unwrap();
        write_file(&db, alice, "demo", "a.md", "secret").await.unwrap();

        // Bob sees nothing of Alice's.
        assert!(list_projects(&db, bob).await.unwrap().is_empty());
        assert!(matches!(
            read_file(&db, bob, "demo", "a.md").await,
            Err(AppError::NotFound)
        ));
        // Same project name is fine — scoped per profile.
        create_project(&db, bob, "demo").await.unwrap();
    }

    #[tokio::test]
    async fn resolve_profile_links_sub_to_email() {
        let db = Db::open_in_memory().unwrap();
        // First seen via forward-auth (sub == email).
        let a = resolve_profile(&db, "x@y.z", "x@y.z").await.unwrap();
        // Later via OIDC with a real sub but the same email → same profile.
        let b = resolve_profile(&db, "uuid-123", "x@y.z").await.unwrap();
        assert_eq!(a, b);
        let c = resolve_profile(&db, "uuid-123", "x@y.z").await.unwrap();
        assert_eq!(a, c);
    }

    #[tokio::test]
    async fn import_legacy_reads_order_sidecar() {
        let tmp = tempfile::tempdir().unwrap();
        let proj = tmp.path().join("ants");
        std::fs::create_dir(&proj).unwrap();
        std::fs::write(proj.join("b.md"), "bee").unwrap();
        std::fs::write(proj.join("a.md"), "ant").unwrap();
        std::fs::write(proj.join(".order"), r#"["b.md","a.md"]"#).unwrap();

        let db = Db::open_in_memory().unwrap();
        let n = import_legacy(&db, tmp.path(), "x@y.z").await.unwrap();
        assert_eq!(n, 1);
        let pid = resolve_profile(&db, "x@y.z", "x@y.z").await.unwrap();
        let files = list_files(&db, pid, "ants").await.unwrap();
        assert_eq!(
            files.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(),
            ["b.md", "a.md"]
        );
        // Second run skips the existing project.
        assert_eq!(import_legacy(&db, tmp.path(), "x@y.z").await.unwrap(), 0);
    }
}
