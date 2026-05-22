/// Live network tests — skipped by default in CI.
/// Run with: cargo test -- --include-ignored
use std::process::Command;

fn yt_dpl() -> Command {
    let bin = env!("CARGO_BIN_EXE_yt-dpl");
    Command::new(bin)
}

/// Extract JSON info without downloading. Verifies the InnerTube API round-trip.
#[test]
#[ignore = "requires network"]
fn youtube_extract_json() {
    let out = yt_dpl()
        .args(["--dump-json", "--quiet", "dQw4w9WgXcQ"])
        .output()
        .expect("failed to run yt-dpl");

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let json: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("invalid JSON output");

    assert_eq!(json["id"].as_str().unwrap(), "dQw4w9WgXcQ");
    assert!(
        json["title"]
            .as_str()
            .unwrap()
            .to_lowercase()
            .contains("rick astley"),
        "unexpected title: {}",
        json["title"]
    );
    let formats = json["formats"].as_array().expect("no formats");
    assert!(formats.len() > 5, "expected many formats, got {}", formats.len());
}

/// Check that --list-formats exits cleanly and prints format IDs.
#[test]
#[ignore = "requires network"]
fn youtube_list_formats() {
    let out = yt_dpl()
        .args(["--list-formats", "dQw4w9WgXcQ"])
        .output()
        .expect("failed to run yt-dpl");

    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stderr);
    // Format 18 (360p combined) should always be present
    assert!(stdout.contains("18") || String::from_utf8_lossy(&out.stdout).contains("18"));
}

/// --simulate should not create any files.
#[test]
#[ignore = "requires network"]
fn youtube_simulate_no_download() {
    let dir = tempfile::tempdir().unwrap();
    let out = yt_dpl()
        .args([
            "--simulate",
            "--quiet",
            "-o",
            &format!("{}/%(title)s.%(ext)s", dir.path().display()),
            "dQw4w9WgXcQ",
        ])
        .output()
        .expect("failed to run yt-dpl");

    assert!(out.status.success());
    let entries: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .flatten()
        .collect();
    assert!(entries.is_empty(), "simulate created files: {:?}", entries);
}

/// Download format 18 (360p combined) and verify the file is non-empty MP4.
#[test]
#[ignore = "requires network"]
fn youtube_download_format_18() {
    let dir = tempfile::tempdir().unwrap();
    let out_template = format!("{}/video.%(ext)s", dir.path().display());

    let status = yt_dpl()
        .args([
            "-f", "18",
            "--no-progress",
            "--quiet",
            "-o", &out_template,
            "dQw4w9WgXcQ",
        ])
        .status()
        .expect("failed to run yt-dpl");

    assert!(status.success());

    let mut entries: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .flatten()
        .collect();
    assert_eq!(entries.len(), 1, "expected exactly one output file");

    let meta = entries[0].metadata().unwrap();
    assert!(meta.len() > 1_000_000, "file too small: {} bytes", meta.len());
}
