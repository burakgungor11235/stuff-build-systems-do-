use crate::bs::cas::Cas;
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

        let cache_dir = project.cache_dir_path();
        let cas = Cas::load(&cache_dir).unwrap_or_default();
        let src_dir = project.src_dir_path();

        let mut files_total = 0u32;
        let mut files_cached = 0u32;
        let mut files_uncached = 0u32;

        for entry in project.walk_source_files() {
            files_total += 1;
            let rel_path = entry
                .path()
                .strip_prefix(&src_dir)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();

            if let Ok((_content, content_hash)) = Cas::read_and_hash(entry.path()) {
                let key = Cas::compute_chunk_key(&rel_path, 0, &content_hash);
                if cas.contains(&key) {
                    files_cached += 1;
                } else {
                    files_uncached += 1;
                }
            } else {
                files_uncached += 1;
            }
        }

        println!("-----------------------------------");
        println!(
            "Files: {} total | {} cached | {} uncached",
            files_total, files_cached, files_uncached
        );

        if verbosity >= 1 {
            println!();
            let manifest_dir = &self.manifest.project.manifest_dir;
            let src_dir_abs = project.src_dir_path();

            // Writing the entire system was easier than this BS.
            for entry in project.walk_source_files() {
                let source_path = entry.path();
                let source_rel = source_path
                    .strip_prefix(manifest_dir)
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| source_path.to_string_lossy().to_string());

                let rel_within_src = source_path
                    .strip_prefix(&src_dir_abs)
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default();

                let output_path = project.output_path_for_source(&rel_within_src);
                let output_exists = manifest_dir.join(&output_path).exists();

                let is_cached =
                    if let Ok((_content, content_hash)) = Cas::read_and_hash(source_path) {
                        let key = Cas::compute_chunk_key(&rel_within_src, 0, &content_hash);
                        cas.contains(&key)
                    } else {
                        false
                    };

                let (color, marker) = if output_exists && is_cached {
                    ("\x1b[32m", "[+]")
                } else if output_exists {
                    ("\x1b[33m", "[*]")
                } else {
                    ("\x1b[31m", "[-]")
                };

                let reset = "\x1b[0m";
                println!(
                    "{}{}{} {} -> {}",
                    color,
                    marker,
                    reset,
                    source_rel,
                    output_path.to_string_lossy()
                );
            }
        }

        if verbosity >= 2 {
            println!();
            println!("CAS entries:");
            for (key, meta) in cas.iter_entries() {
                println!("  {}: {} bytes ({})", key, meta.size, meta.artifact_type);
            }
        }

        Ok(())
    }
}
