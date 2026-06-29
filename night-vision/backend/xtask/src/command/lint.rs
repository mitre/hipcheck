use crate::workspace::get_workspace_path;
use anyhow::{Context as _, Result, anyhow, bail};
use cargo_manifest::{Manifest, MaybeInherited, Publish};
use clap::ArgMatches;
use itertools::Itertools as _;
use pathbuf::pathbuf;
use std::ops::Not as _;

pub fn lint(_args: &ArgMatches) -> Result<()> {
    let mut problems = vec![];

    for manifest in get_workspace_manifests()? {
        apply_checks(&manifest, &mut problems)?;
    }

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

    if let Some(pkg) = &manifest.package
        && manifest.inherits_workspace_lints().not()
    {
        problems.push(anyhow!(
            "'{}' does not inherit workspace lint configuration with `[lints] workspace = true`",
            pkg.name
        ));
    }

    Ok(())
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
}
