use crate::workspace::get_workspace_path;
use anyhow::{Context as _, Result, anyhow, bail};
use cargo_manifest::{Manifest, MaybeInherited, Publish};
use clap::ArgMatches;
use itertools::Itertools as _;
use pathbuf::pathbuf;
use std::ops::Not as _;
use std::path::PathBuf;
use walkdir::WalkDir;

pub fn lint(_args: &ArgMatches) -> Result<()> {
    let mut problems = vec![];
    let manifests = get_workspace_manifests()?;

    for manifest in &manifests {
        apply_checks(manifest, &mut problems)?;
    }

    check_unregistered(
        &get_workspace_manifest_paths()?,
        &find_all_cargo_tomls()?,
        &mut problems,
    )?;

    if problems.is_empty().not() {
        bail!("{}", problems.into_iter().map(|e| e.to_string()).join("\n"))
    }

    Ok(())
}

/// Run any checks we want to apply to package manifests.
fn apply_checks(manifest: &Manifest, problems: &mut Vec<anyhow::Error>) -> Result<()> {
    if let Some(pkg) = &manifest.package
        && pkg.can_publish()
    {
        problems.push(anyhow!(
            "'{}' does not have `publish = false` in its `Cargo.toml`",
            pkg.name
        ));
    }

    // Progenitor owns the implementation included by `nv-server-client`.
    // It cannot satisfy the workspace's hand-written-code lint policy.
    if let Some(pkg) = &manifest.package
        && pkg.name != "nv-server-client"
        && manifest.inherits_workspace_lints().not()
    {
        problems.push(anyhow!(
            "'{}' does not inherit workspace lint configuration with `[lints] workspace = true`",
            pkg.name
        ));
    }

    Ok(())
}

// Walk the workspace directory tree to find all `Cargo.toml` files, including those that are excluded from the workspace.
pub fn find_all_cargo_tomls() -> Result<Vec<PathBuf>> {
    let root = crate::workspace::get_workspace_path()?;
    let mut tomls = Vec::new();
    let root_manifest = root.join("Cargo.toml");

    for entry in WalkDir::new(root)
        .into_iter()
        .filter_map(std::result::Result::ok)
    {
        if entry.file_type().is_file()
            && entry.file_name() == "Cargo.toml"
            && entry.path() != root_manifest
        {
            tomls.push(entry.path().to_path_buf());
        }
    }

    Ok(tomls)
}

/// Run any checks we want to apply to package manifests.
fn check_unregistered(
    registered_manifest_paths: &[PathBuf],
    toml_files: &[PathBuf],
    problems: &mut Vec<anyhow::Error>,
) -> Result<()> {
    let mut missing = vec![];

    for toml in toml_files {
        if !registered_manifest_paths.contains(toml) {
            missing.push(toml.display().to_string());
        }
    }

    if missing.is_empty().not() {
        problems.push(anyhow!(
            "The following packages are not registered in the workspace: {}",
            missing.join(", ")
        ));
    }

    Ok(())
}

/// Get manifest paths for every package listed in the workspace, including excluded packages.
fn get_workspace_manifest_paths() -> Result<Vec<PathBuf>> {
    let workspace_root = get_workspace_path()?;
    let root_manifest = Manifest::from_path(&pathbuf![&workspace_root, "Cargo.toml"])
        .context("failed to load root manifest")?;
    let Some(workspace) = root_manifest.workspace else {
        bail!("root manifest missing `[workspace]` section")
    };

    Ok(workspace
        .members
        .iter()
        .chain(workspace.exclude.as_ref().into_iter().flatten())
        .map(|member| pathbuf![&workspace_root, member, "Cargo.toml"])
        .collect())
}

// Get package manifests for every package in the workspace.
fn get_workspace_manifests() -> Result<Vec<Manifest>> {
    // Get the workspace root manifest.
    let workspace_root = get_workspace_path()?;
    let root_manifest = Manifest::from_path(&pathbuf![&workspace_root, "Cargo.toml"])
        .context("failed to load root manifest")?;

    // Get the names of crates in the workspace.
    let Some(workspace) = root_manifest.workspace else {
        bail!("root manifest missing `[workspace]` section")
    };

    // Resolve all the individual crate manifests.
    let mut manifests = Vec::with_capacity(workspace.members.len());

    for member in &workspace.members {
        let manifest_path = pathbuf![&workspace_root, member, "Cargo.toml"];
        let manifest = Manifest::from_path(&manifest_path).with_context(|| {
            anyhow!(
                "failed to load member manifest for '{}'",
                manifest_path.display()
            )
        })?;
        manifests.push(manifest);
    }

    Ok(manifests)
}

// Implementing this check as an extension trait for ergonomic reasons.
trait CanPublish {
    fn can_publish(&self) -> bool;
}

impl CanPublish for cargo_manifest::Package {
    /// Check whether the package can be published.
    ///
    /// Technically, `cargo`'s rule is if the version is ommitted, then a package implicitly can't
    /// be published. That's too lax for our liking, so we enforce that publication is explicitly
    /// blocked by setting `publish = false` in each package's `Cargo.toml` file.
    fn can_publish(&self) -> bool {
        match self.publish {
            // `publish = true` or `publish = false`
            Some(MaybeInherited::Local(Publish::Flag(flag))) => flag,
            // `publish = ["registry-name"]`
            Some(
                MaybeInherited::Local(Publish::Registry(_))
            // `publish.workspace = true` or `publish = { workspace = true }`
            | MaybeInherited::Inherited { .. },
            )
            // No `publish` field specified.
            | None => true,
        }
    }
}

// Implementing this check as an extension trait for ergonomic reasons.
trait InheritsWorkspaceLints {
    fn inherits_workspace_lints(&self) -> bool;
}

impl InheritsWorkspaceLints for Manifest {
    /// Check whether the package inherits workspace lint configuration.
    fn inherits_workspace_lints(&self) -> bool {
        self.lints
            .as_ref()
            .is_some_and(cargo_manifest::MaybeInheritedLintsSet::is_inherited)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(contents: &str) -> Manifest {
        Manifest::from_slice(contents.as_bytes()).expect("test manifest should parse")
    }

    #[test]
    fn inherited_workspace_lints_pass_manifest_checks() {
        let manifest = manifest(
            r#"
[package]
name = "example"
version = "0.1.0"
publish = false

[lints]
workspace = true
"#,
        );
        let mut problems = vec![];

        apply_checks(&manifest, &mut problems).expect("manifest checks should run");

        assert!(problems.is_empty());
    }

    #[test]
    fn missing_workspace_lints_fail_manifest_checks() {
        let manifest = manifest(
            r#"
[package]
name = "example"
version = "0.1.0"
publish = false
"#,
        );
        let mut problems = vec![];

        apply_checks(&manifest, &mut problems).expect("manifest checks should run");

        assert_eq!(problems.len(), 1);
        assert_eq!(
            problems[0].to_string(),
            "'example' does not inherit workspace lint configuration with `[lints] workspace = true`"
        );
    }

    #[test]
    fn local_lints_without_workspace_inheritance_fail_manifest_checks() {
        let manifest = manifest(
            r#"
[package]
name = "example"
version = "0.1.0"
publish = false

[lints.clippy]
dbg_macro = "deny"
"#,
        );
        let mut problems = vec![];

        apply_checks(&manifest, &mut problems).expect("manifest checks should run");

        assert_eq!(problems.len(), 1);
        assert_eq!(
            problems[0].to_string(),
            "'example' does not inherit workspace lint configuration with `[lints] workspace = true`"
        );
    }

    #[test]
    fn find_all_cargo_tomls_excludes_the_workspace_manifest() {
        let workspace_root = get_workspace_path().expect("workspace root should be found");
        let toml_files = find_all_cargo_tomls().expect("should find package manifests");

        assert!(!toml_files.contains(&workspace_root.join("Cargo.toml")));
        assert!(
            toml_files
                .iter()
                .all(|path| path.file_name().is_some_and(|name| name == "Cargo.toml"))
        );
    }

    #[test]
    fn unregistered_package_pass_manifest_checks() {
        let registered_manifest_paths = vec![PathBuf::from("backend/nvdb/Cargo.toml")];
        let toml_files = registered_manifest_paths.clone();
        let mut problems = vec![];
        check_unregistered(&registered_manifest_paths, &toml_files, &mut problems)
            .expect("manifest checks should run");

        assert!(problems.is_empty());
    }

    #[test]
    fn unregistered_package_fail_manifest_checks() {
        let registered_manifest_paths = vec![PathBuf::from("backend/nvdb/Cargo.toml")];
        let toml_files = vec![
            PathBuf::from("backend/nvdb/Cargo.toml"),
            PathBuf::from("backend/unregistered/Cargo.toml"),
        ];
        let mut problems = vec![];
        check_unregistered(&registered_manifest_paths, &toml_files, &mut problems)
            .expect("manifest checks should run");

        assert_eq!(problems.len(), 1);
    }

    #[test]
    fn unregistered_package_with_registered_suffix_fails_manifest_checks() {
        let registered_manifest_paths = vec![PathBuf::from("backend/nvdb/Cargo.toml")];
        let toml_files = vec![
            PathBuf::from("backend/nvdb/Cargo.toml"),
            PathBuf::from("backend/not-nvdb/Cargo.toml"),
        ];
        let mut problems = vec![];
        check_unregistered(&registered_manifest_paths, &toml_files, &mut problems)
            .expect("manifest checks should run");

        assert_eq!(problems.len(), 1);
        assert_eq!(
            problems[0].to_string(),
            "The following packages are not registered in the workspace: backend/not-nvdb/Cargo.toml"
        );
    }
}
