use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

#[derive(Clone, Copy)]
enum Kind {
    Markdown,
    Asset,
    Skipped,
}

fn collect(root: &Path, dir: &Path, entries: &mut Vec<(PathBuf, Kind)>) -> Result<(), String> {
    let mut children = fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .map(|entry| entry.map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    children.sort_by_key(|entry| entry.file_name());
    for entry in children {
        let path = entry.path();
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        if kind.is_dir() {
            collect(root, &path, entries)?;
        } else {
            let relative = path
                .strip_prefix(root)
                .map_err(|e| e.to_string())?
                .to_path_buf();
            let name = entry.file_name().to_string_lossy().to_lowercase();
            let classification = if !kind.is_file() {
                Kind::Skipped
            } else if name.ends_with(".md") || name.ends_with(".markdown") {
                Kind::Markdown
            } else {
                Kind::Asset
            };
            entries.push((relative, classification));
        }
    }
    Ok(())
}

fn display(path: &Path) -> String {
    path.to_string_lossy()
        .replace(std::path::MAIN_SEPARATOR, "/")
}

fn future_path(path: &Path) -> Result<PathBuf, String> {
    let mut ancestor = path.to_path_buf();
    let mut missing = Vec::new();
    loop {
        match fs::symlink_metadata(&ancestor) {
            Ok(_) => break,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                missing.push(
                    ancestor
                        .file_name()
                        .ok_or("cannot resolve output path")?
                        .to_os_string(),
                );
                ancestor = ancestor
                    .parent()
                    .ok_or("cannot resolve output path")?
                    .to_path_buf();
            }
            Err(error) => return Err(error.to_string()),
        }
    }
    let mut resolved = fs::canonicalize(ancestor).map_err(|e| e.to_string())?;
    for name in missing.iter().rev() {
        resolved.push(name);
    }
    Ok(resolved)
}

pub fn run(source: &str, output: &str, check_loss: bool) -> Result<u8, String> {
    let source =
        fs::canonicalize(source).map_err(|e| format!("cannot open source directory: {e}"))?;
    if !source.is_dir() {
        return Err("source is not a directory".into());
    }
    let requested = PathBuf::from(output);
    let absolute_output = if requested.is_absolute() {
        requested
    } else {
        std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join(requested)
    };
    let output = future_path(&absolute_output)?;
    if fs::symlink_metadata(&output).is_ok() {
        return Err(format!("output already exists: {}", output.display()));
    }
    if output.starts_with(&source) {
        return Err("output must be outside the source directory".into());
    }
    let mut entries = Vec::new();
    collect(&source, &source, &mut entries)?;
    entries.sort_by_key(|(path, _)| display(path).into_bytes());
    if !entries
        .iter()
        .any(|(_, kind)| matches!(kind, Kind::Markdown))
    {
        return Err("source contains no Markdown files".into());
    }
    let parent = output.parent().ok_or("output needs a directory name")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let temporary = parent.join(format!(
        ".{}-{}-{}",
        output.file_name().unwrap().to_string_lossy(),
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos()
    ));
    fs::create_dir(&temporary).map_err(|e| e.to_string())?;
    let result = (|| -> Result<(usize, usize, usize, usize), String> {
        let mut files = Vec::<Value>::new();
        let mut assets = Vec::<String>::new();
        let mut skipped = Vec::<String>::new();
        let mut review = Vec::<String>::new();
        let mut destinations = HashSet::from(["migration-report.json".to_string()]);
        for (relative, kind) in &entries {
            let source_name = display(relative);
            if matches!(kind, Kind::Skipped) {
                skipped.push(source_name);
                continue;
            }
            let destination = if matches!(kind, Kind::Markdown) {
                let name = relative
                    .file_name()
                    .and_then(|name| name.to_str())
                    .ok_or("Markdown filename is not UTF-8")?;
                let suffix = if name.to_lowercase().ends_with(".markdown") {
                    9
                } else {
                    3
                };
                relative.with_file_name(format!("{}.crv", &name[..name.len() - suffix]))
            } else {
                relative.clone()
            };
            let destination_name = display(&destination);
            if !destinations.insert(destination_name.to_lowercase()) {
                return Err(format!(
                    "two inputs map to the same output: {destination_name}"
                ));
            }
            let target = temporary.join(&destination);
            fs::create_dir_all(target.parent().ok_or("invalid output path")?)
                .map_err(|e| e.to_string())?;
            if matches!(kind, Kind::Asset) {
                fs::copy(source.join(relative), target).map_err(|e| e.to_string())?;
                assets.push(source_name);
            } else {
                let bytes = fs::read(source.join(relative)).map_err(|e| e.to_string())?;
                let text = String::from_utf8(bytes)
                    .map_err(|_| format!("invalid UTF-8 in {source_name}"))?;
                let migrated = carve::try_migrate_markdown(&text)
                    .map_err(|e| format!("{source_name}: {e}"))?;
                fs::write(target, migrated.value).map_err(|e| e.to_string())?;
                let report: Value =
                    serde_json::from_str(&super::migration_report_json(&migrated.report))
                        .map_err(|e| e.to_string())?;
                let diagnostics = report["diagnostics"]
                    .as_array()
                    .ok_or("invalid migration report")?
                    .clone();
                if diagnostics
                    .iter()
                    .any(|row| matches!(row["fidelity"].as_str(), Some("degraded" | "dropped")))
                {
                    review.push(source_name.clone());
                }
                files.push(json!({"source": source_name, "output": destination_name, "diagnostics": diagnostics}));
            }
        }
        let counts = (files.len(), assets.len(), review.len(), skipped.len());
        let report = json!({"schemaVersion": 1, "sourceFormat": "markdown", "files": files, "assets": assets, "skipped": skipped, "review": review});
        fs::write(
            temporary.join("migration-report.json"),
            format!(
                "{}\n",
                serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
            ),
        )
        .map_err(|e| e.to_string())?;
        if output.exists() {
            return Err(format!(
                "output appeared during conversion: {}",
                output.display()
            ));
        }
        fs::rename(&temporary, &output).map_err(|e| e.to_string())?;
        Ok(counts)
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&temporary);
    }
    let (files, assets, review, skipped) = result?;
    println!("{files} file(s) converted; {assets} asset(s) copied; {review} file(s) need review; {skipped} entry(s) skipped. Report: {}", output.join("migration-report.json").display());
    Ok(if check_loss && (review > 0 || skipped > 0) {
        1
    } else {
        0
    })
}

#[cfg(test)]
mod tests {
    use super::run;
    use std::fs;

    #[test]
    fn converts_a_tree_and_refuses_overwrites_and_collisions() {
        let root = std::env::temp_dir().join(format!(
            "carve-folder-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let source = root.join("source");
        let output = root.join("output");
        fs::create_dir_all(source.join("nested")).unwrap();
        fs::write(source.join("nested/clean.md"), "# Clean\n").unwrap();
        fs::write(
            source.join("loss.markdown"),
            "A footnote[^a].\n\n[^a]: Note.\n",
        )
        .unwrap();
        fs::write(source.join("asset.svg"), "<svg/>").unwrap();
        assert_eq!(
            run(source.to_str().unwrap(), output.to_str().unwrap(), true).unwrap(),
            1
        );
        assert!(fs::read_to_string(output.join("nested/clean.crv"))
            .unwrap()
            .contains("# Clean"));
        assert_eq!(
            fs::read_to_string(output.join("asset.svg")).unwrap(),
            "<svg/>"
        );
        let report: serde_json::Value =
            serde_json::from_slice(&fs::read(output.join("migration-report.json")).unwrap())
                .unwrap();
        assert_eq!(report["files"].as_array().unwrap().len(), 2);
        assert_eq!(report["review"], serde_json::json!(["loss.markdown"]));
        assert!(
            run(source.to_str().unwrap(), output.to_str().unwrap(), false)
                .unwrap_err()
                .contains("already exists")
        );
        let nested = source.join("new/out");
        assert!(
            run(source.to_str().unwrap(), nested.to_str().unwrap(), false)
                .unwrap_err()
                .contains("outside")
        );
        assert!(!source.join("new").exists());
        let traversal = source.join("../source/new/out");
        assert!(
            run(source.to_str().unwrap(), traversal.to_str().unwrap(), false)
                .unwrap_err()
                .contains("outside")
        );
        assert!(!source.join("new").exists());
        #[cfg(unix)]
        {
            let alias = root.join("alias");
            std::os::unix::fs::symlink(&source, &alias).unwrap();
            let through_alias = alias.join("new/out");
            assert!(run(
                source.to_str().unwrap(),
                through_alias.to_str().unwrap(),
                false
            )
            .unwrap_err()
            .contains("outside"));
            assert!(!source.join("new").exists());
        }
        fs::write(source.join("migration-report.json"), "source").unwrap();
        let collision = root.join("collision");
        assert!(
            run(source.to_str().unwrap(), collision.to_str().unwrap(), false)
                .unwrap_err()
                .contains("same output")
        );
        assert!(!collision.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn dot_md_is_converted_as_a_document() {
        let root = std::env::temp_dir().join(format!(
            "carve-dot-md-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let source = root.join("source");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join(".md"), "# Dot\n").unwrap();
        let output = root.join("output");
        assert_eq!(
            run(source.to_str().unwrap(), output.to_str().unwrap(), true).unwrap(),
            0
        );
        assert!(output.join(".crv").exists());
        fs::remove_dir_all(root).unwrap();
    }
}
