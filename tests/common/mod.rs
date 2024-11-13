use std::fs;

#[allow(dead_code)]
pub fn teardown() {
    glob::glob("tests/fixtures/*/tmp/cache/codeowners")
        .expect("Failed to read glob pattern")
        .filter_map(Result::ok)
        .for_each(|cache_dir| {
            if let Err(err) = fs::remove_dir_all(&cache_dir) {
                eprintln!("Failed to remove {} during test teardown: {}", &cache_dir.display(), err);
            }
        });
}
