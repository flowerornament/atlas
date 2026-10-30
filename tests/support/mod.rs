use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
static NEXT: AtomicUsize = AtomicUsize::new(0);
pub struct Fixture(pub PathBuf);
impl Fixture {
    pub fn new(files: &[(&str, &str)]) -> Self {
        let path = std::env::temp_dir().join(format!(
            "atlas-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).expect("exclusive fixture directory");
        let f = Self(path);
        f.write(
            "Cargo.toml",
            "[package]\nname='fixture'\nversion='0.1.0'\nedition='2024'\n[workspace]\n",
        );
        for (p, text) in files {
            f.write(p, text);
        }
        f
    }
    pub fn write(&self, path: &str, text: &str) {
        let p = self.0.join(path);
        fs::create_dir_all(p.parent().expect("test fixture prerequisite must succeed"))
            .expect("test fixture prerequisite must succeed");
        fs::write(p, text).expect("test fixture prerequisite must succeed");
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
