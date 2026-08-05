use std::fmt;
use std::str::FromStr;

/// A validated npm package name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NpmPackageName(String);

impl NpmPackageName {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for NpmPackageName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for NpmPackageName {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err("Package name length must be greater than zero");
        }
        if s.len() > 214 {
            return Err("Package name length cannot exceed 214 characters");
        }
        if s.chars().any(char::is_whitespace) {
            return Err("Package name should not contain any spaces");
        }

        let (scope, package) = if let Some(scoped_name) = s.strip_prefix('@') {
            let Some((scope, package)) = scoped_name.split_once('/') else {
                return Err("Scoped package names must use the form @scope/name");
            };
            if package.contains('/') {
                return Err("Scoped package names must use the form @scope/name");
            }

            (Some(scope), package)
        } else {
            if s.contains('/') {
                return Err("Unscoped package names cannot contain a slash");
            }

            (None, s)
        };

        if !is_valid_package_name_component(package)
            || scope.is_some_and(|scope| !is_valid_package_name_component(scope))
        {
            return Err("Package name components must be lowercase URL-safe names");
        }

        if matches!(
            s,
            "node_modules" | "favicon.ico" | "http" | "stream" | "fs" | "path"
        ) {
            return Err("Package name cannot conflict with Node core modules or reserved paths");
        }

        Ok(NpmPackageName(s.to_string()))
    }
}

fn is_valid_package_name_component(component: &str) -> bool {
    !component.is_empty()
        && !component.starts_with(['.', '_'])
        && component.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_' | b'.')
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_unscoped_and_scoped_package_names() {
        for name in [
            "package",
            "package-name",
            "package_name",
            "package.name",
            "a1",
            "0package",
            "@scope/package",
            "@scope/package-name",
            "@scope_2/package.name",
            "@a/b",
            "@npm/cli",
        ] {
            let parsed = NpmPackageName::from_str(name).expect("valid package name");
            assert_eq!(parsed.as_str(), name);
        }
    }

    #[test]
    fn rejects_malformed_package_names() {
        for name in [
            "",
            " package",
            "package ",
            "package\tname",
            ".package",
            "_package",
            "UPPERCASE",
            "package/child",
            "package/child/grandchild",
            "package/",
            "/package",
            "@scope",
            "@",
            "@scope/",
            "@/package",
            "@scope/package/child",
            "@scope//package",
            "@@scope/package",
            "@scope/@package",
            "@scope/._package",
            "package~name",
            "package!name",
            "package*name",
            "package(name)",
            "package'name",
            "package?name",
            "package%name",
            "package:name",
            "package\\name",
            "café",
            "node_modules",
            "favicon.ico",
            "http",
            "stream",
            "fs",
            "path",
        ] {
            assert!(
                NpmPackageName::from_str(name).is_err(),
                "{name:?} should be rejected"
            );
        }
    }

    #[test]
    fn rejects_package_names_over_the_npm_length_limit() {
        let name = "a".repeat(215);

        assert!(NpmPackageName::from_str(&name).is_err());
    }
}
