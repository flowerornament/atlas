//! Shared RAII directory ownership, independent of any test's project model.
use std::{fs, path::Path};

pub struct Fixture(tempfile::TempDir);
impl Fixture {
    pub fn new(files: &[(&str, &str)]) -> Self {
        let f = Self(tempfile::tempdir().expect("create core fixture directory"));
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
        fs::create_dir_all(path.parent().expect("core fixture file parent"))
            .expect("create core fixture parent directories");
        fs::write(path, text).expect("write core fixture file");
    }
}
