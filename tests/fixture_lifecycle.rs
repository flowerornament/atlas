//! Directory lifetime is independent of project examples and assertion meaning.
mod support;
use support::Fixture;

#[test]
fn fixtures_are_unique_in_parallel_and_removed_on_drop_or_unwind() {
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(4));
    let threads: Vec<_> = (0..4)
        .map(|_| {
            let barrier = std::sync::Arc::clone(&barrier);
            std::thread::spawn(move || {
                let fixture = Fixture::new(&[("src/lib.rs", "pub struct Example;\n")]);
                let path = fixture.path().to_owned();
                assert!(path.join("src/lib.rs").is_file());
                barrier.wait(); // All four directories are alive simultaneously.
                drop(fixture);
                assert!(!path.exists());
                path
            })
        })
        .collect();
    let paths: Vec<_> = threads
        .into_iter()
        .map(|thread| thread.join().unwrap())
        .collect();
    let unique: std::collections::BTreeSet<_> = paths.iter().collect();
    assert_eq!(unique.len(), paths.len());

    let fixture = Fixture::new(&[]);
    let path = fixture.path().to_owned();
    let result = std::panic::catch_unwind(move || {
        let _owner = fixture;
        panic!("deliberate fixture unwind");
    });
    assert!(result.is_err());
    assert!(!path.exists());
}
