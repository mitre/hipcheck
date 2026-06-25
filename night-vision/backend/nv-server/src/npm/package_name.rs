use std::fmt;
use std::str::FromStr;



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
        let name = s.trim();

        // 1. Length constraint
        if name.is_empty() {
            return Err("Package name length must be greater than zero");
        }
        if name.len() > 214 {
            return Err("Package name length cannot exceed 214 characters");
        }

        // 2. Whitespace check
        if name.contains(' ') {
            return Err("Package name should not contain any spaces");
        }

        // 3. Leading characters
        if name.starts_with('.') || name.starts_with('_') {
            return Err("Package name should not start with . or _");
        }

        // 4. Lowercase constraint
        if name.chars().any(|c| c.is_uppercase()) {
            return Err("Package name must be all lowercase");
        }

        // 5. Illegal character set
        if name.chars().any(|c| matches!(c, '~' | ')' | '(' | '\'' | '!' | '*')) {
            return Err("Package name should not contain any of the following characters: ~)('!*");
        }

        // 6. Basic URL safety check (excluding the scope markers '@' and '/')
        for c in name.chars() {
            if c != '@' && c != '/' && c != '-' && c != '.' && c != '_' && !c.is_alphanumeric() {
                return Err("Package name must contain only URL-safe characters");
            }
        }

        // 7. Core node modules exclusions
        let lower = name.to_lowercase();
        if matches!(lower.as_str(), "node_modules" | "favicon.ico" | "http" | "stream" | "fs" | "path") {
            return Err("Package name cannot conflict with Node core modules or reserved paths");
        }

        Ok(NpmPackageName(name.to_string()))
    }
}
