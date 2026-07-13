//! Documentation update functionality for project-lore and lore-mcp.

use std::path::PathBuf;
use std::process::Command;

/// Detects project type and runs appropriate documentation generation command.
/// Returns a String describing the result for use in both CLI and MCP contexts.
pub fn update_documentation() -> String {
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
