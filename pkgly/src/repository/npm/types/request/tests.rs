#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use nr_core::storage::StoragePath;

use super::GetPath;
#[test]
pub fn tests() {
    let tests = vec![
        (
            StoragePath::from("@nr/mylib/-/@nr/mylib-1.0.0.tgz"),
            GetPath::GetTar {
                name: "@nr/mylib".to_string(),
                version: "1.0.0".to_string(),
                file: "mylib-1.0.0.tgz".to_string(),
            },
        ),
        (
            StoragePath::from("mylib/-/mylib-1.0.0.tgz"),
            GetPath::GetTar {
                name: "mylib".to_string(),
                version: "1.0.0".to_string(),
                file: "mylib-1.0.0.tgz".to_string(),
            },
        ),
        (
            StoragePath::from("mylib/1.0.0"),
            GetPath::VersionInfo {
                name: "mylib".to_string(),
                version: "1.0.0".to_string(),
            },
        ),
        (
            StoragePath::from("mylib"),
            GetPath::GetPackageInfo {
                name: "mylib".to_string(),
            },
        ),
        (
            StoragePath::from("npm-check-updates/-/npm-check-updates-11.0.3.tgz"),
            GetPath::GetTar {
                name: "npm-check-updates".to_string(),
                version: "11.0.3".to_string(),
                file: "npm-check-updates-11.0.3.tgz".to_string(),
            },
        ),
    ];
    for (path, expected) in tests {
        let get_path = GetPath::try_from(path).unwrap();
        assert_eq!(get_path, expected);
    }
}

#[test]
fn prerelease_tarball_paths_keep_the_complete_version() {
    for (path, name, file) in [
        (
            "my-lib/-/my-lib-1.2.3-beta.1.tgz",
            "my-lib",
            "my-lib-1.2.3-beta.1.tgz",
        ),
        (
            "@scope/my-lib/-/my-lib-1.2.3-beta.1.tgz",
            "@scope/my-lib",
            "my-lib-1.2.3-beta.1.tgz",
        ),
        (
            "@scope/my-lib/-/@scope/my-lib-1.2.3-beta.1.tgz",
            "@scope/my-lib",
            "my-lib-1.2.3-beta.1.tgz",
        ),
    ] {
        assert_eq!(
            GetPath::try_from(StoragePath::from(path)).unwrap(),
            GetPath::GetTar {
                name: name.into(),
                version: "1.2.3-beta.1".into(),
                file: file.into(),
            }
        );
    }
}

#[test]
fn tarball_version_parser_handles_prereleases_and_build_metadata() {
    for version in [
        "1.2.3",
        "1.2.3-beta.1",
        "1.2.3-rc-test.2+build.4",
        "1.2.3-20261016",
    ] {
        let path = StoragePath::from(format!("my-lib/-/my-lib-{version}.tgz"));
        let GetPath::GetTar {
            version: actual, ..
        } = GetPath::try_from(path).unwrap()
        else {
            panic!("expected tarball");
        };
        assert_eq!(actual, version);
    }
    for file in [
        "my-lib.tgz",
        "my-lib-beta.1.tgz",
        "my-lib-1.2.3.zip",
        "my-lib-1.2.3.tgz.tgz",
    ] {
        assert!(
            GetPath::try_from(StoragePath::from(format!("my-lib/-/{file}"))).is_err(),
            "{file}"
        );
    }
}

#[test]
fn version_like_package_name_does_not_change_the_tarball_version() {
    let path = StoragePath::from("lib-1.0.0/-/lib-1.0.0-2.0.0-beta.1.tgz");
    let GetPath::GetTar { version, .. } = GetPath::try_from(path).unwrap() else {
        panic!("expected tarball");
    };
    assert_eq!(version, "2.0.0-beta.1");
}
