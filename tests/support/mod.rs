//! Owned fixture directories; source examples and assertions stay with readers.
use std::{fs, path::Path};

pub struct Fixture(tempfile::TempDir);
impl Fixture {
    pub fn new(files: &[(&str, &str)]) -> Self {
        let f = Self(tempfile::tempdir().expect("create fixture directory"));
        f.write(
            "Cargo.toml",
            "[package]\nname='fixture'\nversion='0.1.0'\nedition='2024'\n[workspace]\n",
        );
        for (path, text) in files {
            f.write(path, text);
        }
        f
    }
    pub fn path(&self) -> &Path {
        self.0.path()
    }
    pub fn write(&self, path: &str, text: &str) {
        let path = self.path().join(path);
        fs::create_dir_all(path.parent().expect("fixture file parent"))
            .expect("create fixture parent directories");
        fs::write(path, text).expect("write fixture file");
    }
}
