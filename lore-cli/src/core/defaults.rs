//! Shared default configurations for system-lore, project-lore, and lore-mcp binaries.

use crate::arguments::Cli;

/// Creates a Cli with system documentation defaults
pub fn system_lore_defaults(query: Vec<String>, context: usize) -> Cli {
    Cli {
        query,
        paths: vec![
            "/usr/share/doc".into(),
            "/usr/share/man".into(),
            "/usr/share/info".into(),
            "/usr/share/gir-1.0".into(),
            "/usr/share/gtk-doc/html".into(),
            "/usr/share/devhelp/books".into(),
            "/usr/local/share/doc".into(),
            "/usr/local/share/man".into(),
            "/usr/local/share/gir-1.0".into(),
            "/usr/local/share/gtk-doc/html".into(),
            "/usr/src/linux/Documentation".into(),
            "/usr/share/doc/linux-doc".into(),
        ],
        exclude_path_patterns: vec![
            "/usr/share/locale".to_string(),
            "/usr/local/share/locale".to_string(),
            "/usr/lib/locale".to_string(),
            "/usr/share/i18n".to_string(),
            "/usr/share/icons".to_string(),
            "/usr/share/fonts".to_string(),
            "/usr/share/mime".to_string(),
            "/var/cache".to_string(),
            "/tmp".to_string(),
            "translated".to_string(),
            "fonts".to_string(),
        ],
        exclude_match: vec!["translation status".to_string()],
        exclude_extensions: vec![
            "css".to_string(),
            "js".to_string(),
            "map".to_string(),
            "svg".to_string(),
            "png".to_string(),
            "jpg".to_string(),
            "jpeg".to_string(),
            "gif".to_string(),
            "ico".to_string(),
        ],
        include_path: true,
        walk_yield_interval: 1024,
        workers: 1,
        context: Some(context),
        stream: false,
    }
}

/// Creates a Cli with project documentation defaults
pub fn project_lore_defaults(query: Vec<String>, context: usize) -> Cli {
    Cli {
        query,
        paths: vec![
            "target/doc".into(),
            "./doc".into(),
            "build/html".into(),
            "build/docs".into(),
            "docs/html".into(),
            "docs/_build/html".into(),
            "documentation/html".into(),
            "docs".into(),
            "documentation".into(),
            "doc".into(),
            ".cargo/doc".into(),
        ],
        exclude_path_patterns: vec![
            "node_modules".to_string(),
            "target/debug".to_string(),
            "target/release".to_string(),
            ".git".to_string(),
            ".svn".to_string(),
            ".cache".to_string(),
            "tmp".to_string(),
            "temp".to_string(),
            ".vscode".to_string(),
            ".idea".to_string(),
            "dist".to_string(),
            "build/CMakeFiles".to_string(),
        ],
        exclude_match: vec![],
        exclude_extensions: vec![
            "css".to_string(),
            "js".to_string(),
            "map".to_string(),
            "woff".to_string(),
            "woff2".to_string(),
            "ttf".to_string(),
            "eot".to_string(),
            "svg".to_string(),
            "png".to_string(),
            "jpg".to_string(),
            "jpeg".to_string(),
            "gif".to_string(),
            "ico".to_string(),
            "webp".to_string(),
            "o".to_string(),
            "a".to_string(),
            "so".to_string(),
            "dylib".to_string(),
            "dll".to_string(),
            "lock".to_string(),
        ],
        include_path: true,
        walk_yield_interval: 1024,
        workers: 1,
        context: Some(context),
        stream: false,
    }
}

/// Detects project type and runs appropriate documentation generation command
pub fn update_documentation() -> String {
    use std::path::PathBuf;
    use std::process::Command;

    let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    // Check for Rust project
    if current_dir.join("Cargo.toml").exists() {
        let output = Command::new("cargo").args(["doc", "--no-deps"]).output();

        match output {
            Ok(output) if output.status.success() => {
                return "✅ Rust documentation generated successfully in target/doc/".to_string();
            }
            Ok(output) => {
                return format!(
                    "❌ cargo doc failed:\n{}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            Err(e) => {
                return format!("❌ Failed to run cargo doc: {}", e);
            }
        }
    }

    // Check for Go project
    if current_dir.join("go.mod").exists() {
        return "🐹 Go project detected. Run 'godoc -http=:6060' to serve documentation locally"
            .to_string();
    }

    // Check for Doxygen
    if current_dir.join("Doxyfile").exists() || current_dir.join("doxygen.conf").exists() {
        let output = Command::new("doxygen").output();

        match output {
            Ok(output) if output.status.success() => {
                return "✅ Doxygen documentation generated successfully".to_string();
            }
            Ok(output) => {
                return format!(
                    "❌ doxygen failed:\n{}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            Err(e) => {
                return format!("❌ Failed to run doxygen: {}", e);
            }
        }
    }

    // Check for Python/Sphinx
    if current_dir.join("docs").join("conf.py").exists() {
        let output = Command::new("sphinx-build")
            .args(["-b", "html", "docs", "docs/_build/html"])
            .output();

        match output {
            Ok(output) if output.status.success() => {
                return "✅ Sphinx documentation generated successfully in docs/_build/html/"
                    .to_string();
            }
            Ok(output) => {
                return format!(
                    "❌ sphinx-build failed:\n{}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            Err(e) => {
                return format!("❌ Failed to run sphinx-build: {}", e);
            }
        }
    }

    "❌ No supported project type detected. Supported: Rust (Cargo.toml), Go (go.mod), Doxygen (Doxyfile), Python/Sphinx (docs/conf.py)".to_string()
}
