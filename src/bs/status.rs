use walkdir::WalkDir;

use crate::bs::cache::{BuildCache, CacheStatus};
use crate::bs::config::Manifest;

pub struct Status {
    manifest: Manifest,
}

impl Status {
    pub fn new(manifest: Manifest) -> Self {
        Self { manifest }
    }

    pub fn show(&self, verbosity: u8) -> anyhow::Result<()> {
        let project = &self.manifest.project;

        println!("Project: {}", project.name);
        println!("  Source: {}", project.src_dir);
        println!("  Output: {}", project.out_dir);
        println!("  Cache: {}", project.cache_dir);

        let cache = BuildCache::load(project.cache_dir_path())?;
        let src_dir = project.src_dir_path();

        let mut files_total = 0u32;
        let mut files_up_to_date = 0u32;
        let mut files_stale = 0u32;
        let mut files_not_cached = 0u32;

        for entry in WalkDir::new(project.src_dir_path())
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .map_or_else(|| false, |ext| ext == "md")
            })
        {
            files_total += 1;
            match cache.is_fresh(&src_dir, entry.path()) {
                CacheStatus::UpToDate => files_up_to_date += 1,
                CacheStatus::Stale => files_stale += 1,
                CacheStatus::NotCached => files_not_cached += 1,
            }
        }

        println!("-----------------------------------");
        println!(
            "Files: {} total | {} up-to-date | {} stale | {} not-cached",
            files_total, files_up_to_date, files_stale, files_not_cached
        );

        if verbosity >= 1 {
            println!();
            let manifest_dir = &self.manifest.project.manifest_dir;
            let src_dir = &self.manifest.project.src_dir;
            let out_dir = &self.manifest.project.out_dir;
            // Walk the source directory (relative to manifest directory)
            let src_dir_abs = manifest_dir.join(src_dir);
            for entry in WalkDir::new(&src_dir_abs)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.path()
                        .extension()
                        .map_or_else(|| false, |ext| ext == "md")
                })
            {
                let source_path = entry.path();
                // Get the path of the source file relative to the manifest directory
                let source_rel = match source_path.strip_prefix(manifest_dir) {
                    Ok(p) => p.to_string_lossy().to_string(),
                    Err(_) => {
                        // Fallback: should not happen if we walked from src_dir_abs which is inside manifest_dir
                        source_path.to_string_lossy().to_string()
                    }
                };
                // The relative path within the source directory (for constructing output path)
                let rel_within_src = match source_path.strip_prefix(&src_dir_abs) {
                    Ok(p) => p.to_string_lossy().to_string(),
                    Err(_) => {
                        // Fallback: use the whole source_rel (should not happen)
                        source_rel.clone()
                    }
                };
                // Expected output file relative to manifest directory: out_dir / rel_within_src with .html extension
                let mut output_rel = std::path::PathBuf::from(out_dir);
                output_rel.push(&rel_within_src);
                output_rel.set_extension("html");
                // Check if output file exists on disk: manifest_dir / output_rel
                let output_exists = manifest_dir.join(&output_rel).exists();

                let status = cache.is_fresh(&manifest_dir.join(&src_dir), &source_path);

                let color ;
                let marker;
                if output_exists {
                    (color, marker) = match status {
                        CacheStatus::UpToDate => ("\x1b[32m", "[+]"),
                        CacheStatus::Stale => ("\x1b[33m", "[*]"),
                        CacheStatus::NotCached => ("\x1b[31m", "[-]"), // this should be unreachable
                    };
                } else {
                    color = "\x1b[31m";
                    marker = "[-]";
                }

               
                let reset = "\x1b[0m";

                if output_exists {
                    println!(
                        "{}{}{} {} -> {}",
                        color,
                        marker,
                        reset,
                        source_rel,
                        output_rel.to_string_lossy()
                    );
                } else {
                    println!("{}{}{} {}", color, marker, reset, source_rel);
                }
            }
        }

        if verbosity >= 2 {
            println!();
            println!("Cache contents:");
            for (path, mtime) in &cache.entries {
                println!("  {}: {}", path, mtime);
            }
        }

        Ok(())
    }
}

