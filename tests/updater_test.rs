use SteamProcessIdler::updater::is_newer_version;

#[test]
fn test_version_comparisons() {
    assert!(!is_newer_version("1.0.0", "1.1.0"));
    assert!(!is_newer_version("v1.0.0", "1.1.0"));
    assert!(!is_newer_version("1.1.0", "1.1.0"));
    assert!(!is_newer_version("v1.1.0", "1.1.0"));
    assert!(!is_newer_version("1.0.9", "1.1.0"));
    assert!(is_newer_version("1.1.1", "1.1.0"));
    assert!(is_newer_version("v1.1.1", "1.1.0"));
    assert!(is_newer_version("1.2.0", "1.1.0"));
    assert!(is_newer_version("2.0.0", "1.1.0"));
    assert!(is_newer_version("1.1.0.1", "1.1.0"));
}
