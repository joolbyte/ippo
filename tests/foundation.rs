#![cfg(debug_assertions)]

use ippo::{config::DataEnvironment, diagnostics::Diagnostics, storage::Database};

#[test]
fn diagnostics_use_an_injected_disposable_database() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().join("isolated-test.db");
    let database = Database::open(&path, DataEnvironment::Test).expect("test database");

    let diagnostics = Diagnostics::collect(ippo::config::Profile::Dev, &path, true, &database)
        .expect("diagnostics");

    assert_eq!(diagnostics.environment, "test");
    assert_eq!(diagnostics.schema_version, 4);
    assert_eq!(diagnostics.database_path, path.to_string_lossy());
}
