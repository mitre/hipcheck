use nv_common::npm::package_json::{NpmPackageJson, PackageParseError};
use std::fs::File;

pub fn run_npm_parser(api_path: &str) -> Result<(), PackageParseError> {
    let file = File::open(api_path)?;

    let pkg = NpmPackageJson::parse_package_json(file)?;
    println!("Safe package.json: {:#?}", pkg);

    Ok(())
}
