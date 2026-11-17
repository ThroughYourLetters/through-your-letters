use std::{fs, process::Command};

#[test]
fn startup_rejects_malformed_dotenv_without_exposing_its_contents() {
    let directory = std::env::temp_dir().join(format!("ttl-startup-{}", uuid::Uuid::now_v7()));
    fs::create_dir(&directory).unwrap();
    for (contents, expected) in [
        (
            "PRIVATE_SECRET=dotenv-secret-canary\nBROKEN='dotenv-secret-canary",
            "Cannot load .env",
        ),
        ("", "JWT_SECRET"),
    ] {
        fs::write(directory.join(".env"), contents).unwrap();
        let result = Command::new(env!("CARGO_BIN_EXE_api"))
            .current_dir(&directory)
            .env_clear()
            .output()
            .unwrap();
        assert!(!result.status.success());
        let error = String::from_utf8(result.stderr).unwrap();
        assert!(error.contains(expected), "{error}");
        assert!(!error.contains("dotenv-secret-canary"));
    }
    fs::remove_dir_all(directory).unwrap();
}
