use anyhow::{Context as _, Result, bail};
use camino::{Utf8Path, Utf8PathBuf};
use chrono::Utc;
use nv_common::npm::packument::{
    DependencyRegistrySupport, NpmPackument, PackumentDependencyMap, ParsedDependencyPackageName,
    ParsedDependencySpec, dependency_registry_support, parse_packument,
};
use nv_common::npm_semver::{
    NpmVersion as RangeVersion, elaborate_npm_version_bounds, parse_range,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
    fs::{self, File},
    process::Command,
};
use url::Url;

use crate::destructive::DestructiveOperationToken;

const CORPUS_REFRESH_STATE_FILE: &str = "../nv-common/testdata/npm/packument/corpus-refresh.json";
const CORPUS_CATALOG_FILE: &str = "../nv-common/testdata/npm/packument/corpus.json";
const CORPUS_REAL_DIRECTORY: &str = "../nv-common/testdata/npm/packument/real";
const CORPUS_CACHE_DIRECTORY: &str = "../nv-common/testdata/npm/packument/cache";
const MAX_PACKUMENT_BYTES: &str = "67108864";
const DEFAULT_INSPECTION_LIMIT: u64 = 100;
const MAX_INSPECTION_VALUE_CHARS: usize = 1024;

pub fn command() -> clap::Command {
    clap::Command::new("packument")
        .about("Inspect npm packument parser compatibility")
        .arg_required_else_help(true)
        .subcommand(
            clap::Command::new("inspect")
                .about("Print normalized metadata from a local npm packument")
                .arg(
                    clap::Arg::new("file")
                        .required(true)
                        .value_name("FILE")
                        .value_parser(clap::value_parser!(Utf8PathBuf))
                        .help("Local full or abbreviated packument JSON file"),
                )
                .arg(
                    clap::Arg::new("json")
                        .long("json")
                        .action(clap::ArgAction::SetTrue)
                        .help("Print the normalized packument summary as JSON"),
                )
                .arg(
                    clap::Arg::new("limit")
                        .long("limit")
                        .default_value(DEFAULT_INSPECTION_LIMIT.to_string())
                        .value_name("COUNT")
                        .value_parser(clap::value_parser!(u64).range(1..))
                        .help("Maximum versions and metadata entries to display; individual values remain capped"),
                )
                .arg(
                    clap::Arg::new("no-limit")
                        .long("no-limit")
                        .action(clap::ArgAction::SetTrue)
                        .conflicts_with("limit")
                        .help("Display all versions and metadata entries; individual values remain capped"),
                ),
        )
        .subcommand(
            clap::Command::new("resolve")
                .about("List published versions in a local packument that satisfy an npm range")
                .arg(
                    clap::Arg::new("file")
                        .required(true)
                        .value_name("FILE")
                        .value_parser(clap::value_parser!(Utf8PathBuf))
                        .help("Path to a local npm packument JSON file"),
                )
                .arg(
                    clap::Arg::new("range")
                        .required(true)
                        .value_name("RANGE")
                        .help("Npm version range to resolve"),
                ),
        )
        .subcommand(
            clap::Command::new("corpus-add")
                .about("Fetch, validate, and add a reviewed packument fixture")
                .arg(
                    clap::Arg::new("package")
                        .required(true)
                        .value_name("PACKAGE")
                        .help("Public npm package name to add"),
                )
                .arg(
                    clap::Arg::new("destructive")
                        .short('w')
                        .long("destructive")
                        .required(true)
                        .action(clap::ArgAction::SetTrue)
                        .help("Acknowledge this command updates reviewed corpus files"),
                )
                .arg(registry_argument()),
        )
        .subcommand(
            clap::Command::new("corpus-refresh")
                .about("Fetch the curated corpus and write a compatibility summary")
                .arg(
                    clap::Arg::new("destructive")
                        .short('w')
                        .long("destructive")
                        .required(true)
                        .action(clap::ArgAction::SetTrue)
                        .help("Acknowledge this command updates corpus compatibility state"),
                )
                .arg(registry_argument()),
        )
}

fn registry_argument() -> clap::Arg {
    clap::Arg::new("registry")
        .long("registry")
        .default_value("https://registry.npmjs.org/")
        .value_name("URL")
        .value_parser(clap::value_parser!(Url))
        .help("Registry base URL; useful for controlled test registries")
}

pub fn run(matches: &clap::ArgMatches) -> Result<()> {
    if let Some(inspect_matches) = matches.subcommand_matches("inspect") {
        let path = inspect_matches
            .get_one::<Utf8PathBuf>("file")
            .expect("file is required by clap");
        let packument = parse_packument(
            File::open(path).with_context(|| format!("failed to read packument at {path}"))?,
        )
        .with_context(|| format!("failed to parse packument at {path}"))?;
        let limit = (!inspect_matches.get_flag("no-limit"))
            .then(|| {
                *inspect_matches
                    .get_one::<u64>("limit")
                    .expect("limit has a clap default")
            })
            .map(|limit| usize::try_from(limit).context("--limit is too large"))
            .transpose()?;
        let inspection = inspect_packument(&packument, limit);

        if inspect_matches.get_flag("json") {
            println!(
                "{}",
                serde_json::to_string_pretty(&inspection)
                    .expect("packument inspection is always serializable")
            );
        } else {
            print_inspection(&inspection);
        }
        return Ok(());
    }

    if let Some(resolve_matches) = matches.subcommand_matches("resolve") {
        let file = resolve_matches
            .get_one::<Utf8PathBuf>("file")
            .expect("file is required by clap");
        let raw_range = resolve_matches
            .get_one::<String>("range")
            .expect("range is required by clap");

        for version in resolve_packument_versions(file, raw_range)? {
            println!("{version}");
        }
        return Ok(());
    }

    if let Some(add_matches) = matches.subcommand_matches("corpus-add") {
        let token = DestructiveOperationToken::new(add_matches);
        let package = add_matches
            .get_one::<String>("package")
            .expect("package is required by clap");
        let registry = add_matches
            .get_one::<Url>("registry")
            .expect("registry has a clap default");
        return add_corpus_package(registry, package, token);
    }

    if let Some(refresh_matches) = matches.subcommand_matches("corpus-refresh") {
        let token = DestructiveOperationToken::new(refresh_matches);
        let registry = refresh_matches
            .get_one::<Url>("registry")
            .expect("registry has a clap default");
        return refresh_corpus(registry, token);
    }

    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PackumentInspection {
    name: String,
    dist_tags: InspectedStringMap,
    available_versions: usize,
    versions: Vec<InspectedVersion>,
    #[serde(skip_serializing_if = "is_zero")]
    omitted_versions: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InspectedVersion {
    version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    published_at: Option<String>,
    dependencies: InspectedDependencies,
    dist: InspectedDist,
    #[serde(skip_serializing_if = "Option::is_none")]
    deprecated: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    has_install_script: Option<bool>,
    engines: InspectedStringMap,
    cpu: InspectedStringList,
    os: InspectedStringList,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InspectedDependencies {
    dependencies: InspectedDependencyMap,
    dev_dependencies: InspectedDependencyMap,
    peer_dependencies: InspectedDependencyMap,
    optional_dependencies: InspectedDependencyMap,
    bundle_dependencies: InspectedStringList,
}

#[derive(Serialize)]
struct InspectedDependencyMap {
    entries: BTreeMap<String, InspectedDependency>,
    #[serde(skip_serializing_if = "is_zero")]
    omitted: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InspectedDependency {
    specification: String,
    resolution: DependencyResolution,
}

#[derive(Serialize)]
#[serde(rename_all = "kebab-case")]
enum DependencyResolution {
    Resolvable,
    InvalidName,
    InvalidSpecification,
    UnsupportedHistoricName,
    UnsupportedFile,
    UnsupportedGit,
    UnsupportedUrl,
}

#[derive(Serialize)]
struct InspectedStringMap {
    entries: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "is_zero")]
    omitted: usize,
}

#[derive(Serialize)]
struct InspectedStringList {
    entries: Vec<String>,
    #[serde(skip_serializing_if = "is_zero")]
    omitted: usize,
}

#[derive(Serialize)]
struct InspectedDist {
    tarball: String,
    shasum: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    integrity: Option<String>,
}

fn inspect_packument(packument: &NpmPackument, limit: Option<usize>) -> PackumentInspection {
    let published_at = packument
        .time
        .as_ref()
        .map(|times| {
            times
                .versions
                .iter()
                .map(|(version, timestamp)| (version.as_str(), timestamp.to_string()))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let mut parsed_versions = packument.versions.iter().collect::<Vec<_>>();
    parsed_versions.sort_by_key(|(version, _)| *version);
    let available_versions = parsed_versions.len();
    let start = limit.map_or(0, |limit| available_versions.saturating_sub(limit));
    let omitted_versions = start;
    let versions = parsed_versions
        .into_iter()
        .skip(start)
        .map(|(key, version)| {
            let package_version = key.to_string();
            InspectedVersion {
                version: bounded_inspection_value(&package_version),
                published_at: published_at.get(package_version.as_str()).cloned(),
                dependencies: InspectedDependencies {
                    dependencies: inspected_dependency_map(&version.dependencies, limit),
                    dev_dependencies: inspected_dependency_map(&version.dev_dependencies, limit),
                    peer_dependencies: inspected_dependency_map(&version.peer_dependencies, limit),
                    optional_dependencies: inspected_dependency_map(
                        &version.optional_dependencies,
                        limit,
                    ),
                    bundle_dependencies: inspected_string_list(
                        {
                            let mut names = version
                                .bundle_dependencies
                                .as_slice()
                                .iter()
                                .map(|name| bounded_inspection_value(name.as_str()))
                                .collect::<Vec<_>>();
                            names.sort();
                            names
                        },
                        limit,
                    ),
                },
                dist: InspectedDist {
                    tarball: bounded_inspection_value(version.dist.tarball.as_str()),
                    shasum: bounded_inspection_value(version.dist.shasum.as_str()),
                    integrity: version
                        .dist
                        .integrity
                        .as_deref()
                        .map(bounded_inspection_value),
                },
                deprecated: version.deprecated.as_deref().map(bounded_inspection_value),
                has_install_script: version.has_install_script,
                engines: inspected_string_map(
                    version
                        .engines
                        .iter()
                        .map(|(name, range)| (name.to_owned(), bounded_inspection_value(range)))
                        .collect(),
                    limit,
                ),
                cpu: inspected_string_list(
                    sorted_strings(&version.cpu)
                        .iter()
                        .map(|value| bounded_inspection_value(value))
                        .collect(),
                    limit,
                ),
                os: inspected_string_list(
                    sorted_strings(&version.os)
                        .iter()
                        .map(|value| bounded_inspection_value(value))
                        .collect(),
                    limit,
                ),
            }
        })
        .collect::<Vec<_>>();

    PackumentInspection {
        name: packument.name.to_string(),
        dist_tags: inspected_string_map(
            packument
                .dist_tags
                .iter()
                .map(|(tag, version)| {
                    let version = version.to_string();
                    (tag.to_owned(), bounded_inspection_value(&version))
                })
                .collect(),
            limit,
        ),
        available_versions,
        versions,
        omitted_versions,
    }
}

fn inspected_dependency_map(
    dependencies: &PackumentDependencyMap,
    limit: Option<usize>,
) -> InspectedDependencyMap {
    let mut entries = dependencies
        .iter()
        .map(|(name, specification)| {
            let rendered_specification = specification.as_str();
            (
                name.as_str().to_owned(),
                InspectedDependency {
                    specification: bounded_inspection_value(&rendered_specification),
                    resolution: dependency_resolution(name, specification),
                },
            )
        })
        .collect::<Vec<_>>();
    entries.sort_by(|(left_name, _), (right_name, _)| left_name.cmp(right_name));
    let omitted = limit.map_or(0, |limit| entries.len().saturating_sub(limit));
    InspectedDependencyMap {
        entries: render_inspection_keyed_entries(entries, limit),
        omitted,
    }
}

fn dependency_resolution(
    name: &ParsedDependencyPackageName,
    specification: &ParsedDependencySpec,
) -> DependencyResolution {
    let Some(name) = name.valid() else {
        return DependencyResolution::InvalidName;
    };
    let Some(specification) = specification.valid() else {
        return DependencyResolution::InvalidSpecification;
    };

    match dependency_registry_support(name, specification) {
        DependencyRegistrySupport::Resolvable => DependencyResolution::Resolvable,
        DependencyRegistrySupport::UnsupportedFile => DependencyResolution::UnsupportedFile,
        DependencyRegistrySupport::UnsupportedGit => DependencyResolution::UnsupportedGit,
        DependencyRegistrySupport::UnsupportedUrl => DependencyResolution::UnsupportedUrl,
        DependencyRegistrySupport::UnsupportedHistoricName => {
            DependencyResolution::UnsupportedHistoricName
        }
    }
}

fn inspected_string_map(
    entries: BTreeMap<String, String>,
    limit: Option<usize>,
) -> InspectedStringMap {
    let omitted = limit.map_or(0, |limit| entries.len().saturating_sub(limit));
    InspectedStringMap {
        entries: render_inspection_keyed_entries(entries, limit),
        omitted,
    }
}

fn inspected_string_list(entries: Vec<String>, limit: Option<usize>) -> InspectedStringList {
    let omitted = limit.map_or(0, |limit| entries.len().saturating_sub(limit));
    InspectedStringList {
        entries: entries
            .into_iter()
            .take(limit.unwrap_or(usize::MAX))
            .collect(),
        omitted,
    }
}

fn render_inspection_keyed_entries<T>(
    entries: impl IntoIterator<Item = (String, T)>,
    limit: Option<usize>,
) -> BTreeMap<String, T> {
    let mut used_keys = BTreeSet::new();
    let mut rendered = BTreeMap::new();

    for (raw_key, value) in entries.into_iter().take(limit.unwrap_or(usize::MAX)) {
        let rendered_key = unique_inspection_key(&raw_key, &mut used_keys);
        rendered.insert(rendered_key, value);
    }

    rendered
}

fn unique_inspection_key(raw_key: &str, used_keys: &mut BTreeSet<String>) -> String {
    let bounded_key = bounded_inspection_value(raw_key);
    if used_keys.insert(bounded_key.clone()) {
        return bounded_key;
    }

    let mut collision_index: i32 = 2;
    loop {
        let candidate = format!("{bounded_key} [collision {collision_index}]");
        if used_keys.insert(candidate.clone()) {
            return candidate;
        }
        collision_index = collision_index.saturating_add(1);
    }
}

fn is_zero(value: &usize) -> bool {
    *value == 0
}

fn sorted_strings(values: &[String]) -> Vec<String> {
    let mut sorted = values.to_vec();
    sorted.sort();
    sorted
}

fn bounded_inspection_value(value: &str) -> String {
    let Some((end, _)) = value.char_indices().nth(MAX_INSPECTION_VALUE_CHARS) else {
        return value.to_owned();
    };
    let omitted = value[end..].chars().count();
    format!("{}… [{omitted} characters omitted]", &value[..end])
}

fn print_inspection(inspection: &PackumentInspection) {
    print!("{}", format_inspection(inspection));
}

fn format_inspection(inspection: &PackumentInspection) -> String {
    let mut output = String::new();
    writeln!(output, "name: {}", inspection.name).expect("writing to a string cannot fail");
    writeln!(output, "dist-tags:").expect("writing to a string cannot fail");
    for (tag, version) in &inspection.dist_tags.entries {
        writeln!(output, "  {}: {}", text_value(tag), text_value(version))
            .expect("writing to a string cannot fail");
    }
    write_omitted(&mut output, 2, inspection.dist_tags.omitted);
    writeln!(
        output,
        "available versions: {}",
        inspection.available_versions
    )
    .expect("writing to a string cannot fail");
    write_omitted(&mut output, 0, inspection.omitted_versions);
    for version in &inspection.versions {
        writeln!(output, "version: {}", version.version).expect("writing to a string cannot fail");
        if let Some(published_at) = &version.published_at {
            writeln!(output, "  published-at: {published_at}")
                .expect("writing to a string cannot fail");
        }
        write_inspection_map(
            &mut output,
            "dependencies",
            &version.dependencies.dependencies,
        );
        write_inspection_map(
            &mut output,
            "dev-dependencies",
            &version.dependencies.dev_dependencies,
        );
        write_inspection_map(
            &mut output,
            "peer-dependencies",
            &version.dependencies.peer_dependencies,
        );
        write_inspection_map(
            &mut output,
            "optional-dependencies",
            &version.dependencies.optional_dependencies,
        );
        if !version.dependencies.bundle_dependencies.entries.is_empty() {
            writeln!(
                output,
                "  bundle-dependencies: {}",
                version
                    .dependencies
                    .bundle_dependencies
                    .entries
                    .iter()
                    .map(|name| text_value(name))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
            .expect("writing to a string cannot fail");
        }
        write_omitted(
            &mut output,
            2,
            version.dependencies.bundle_dependencies.omitted,
        );
        writeln!(output, "  tarball: {}", version.dist.tarball)
            .expect("writing to a string cannot fail");
        writeln!(output, "  shasum: {}", version.dist.shasum)
            .expect("writing to a string cannot fail");
        if let Some(integrity) = &version.dist.integrity {
            writeln!(output, "  integrity: {}", text_value(integrity))
                .expect("writing to a string cannot fail");
        }
        if let Some(deprecated) = &version.deprecated {
            writeln!(output, "  deprecated: {}", text_value(deprecated))
                .expect("writing to a string cannot fail");
        }
        if let Some(has_install_script) = version.has_install_script {
            writeln!(output, "  has-install-script: {has_install_script}")
                .expect("writing to a string cannot fail");
        }
        write_string_map(&mut output, "engines", &version.engines);
        if !version.cpu.entries.is_empty() {
            writeln!(
                output,
                "  cpu: {}",
                version
                    .cpu
                    .entries
                    .iter()
                    .map(|value| text_value(value))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
            .expect("writing to a string cannot fail");
        }
        write_omitted(&mut output, 2, version.cpu.omitted);
        if !version.os.entries.is_empty() {
            writeln!(
                output,
                "  os: {}",
                version
                    .os
                    .entries
                    .iter()
                    .map(|value| text_value(value))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
            .expect("writing to a string cannot fail");
        }
        write_omitted(&mut output, 2, version.os.omitted);
    }
    output
}

fn write_inspection_map(output: &mut String, label: &str, values: &InspectedDependencyMap) {
    if !values.entries.is_empty() {
        writeln!(output, "  {label}:").expect("writing to a string cannot fail");
        for (name, dependency) in &values.entries {
            writeln!(
                output,
                "    {}: {} ({})",
                text_value(name),
                text_value(&dependency.specification),
                dependency_resolution_label(&dependency.resolution),
            )
            .expect("writing to a string cannot fail");
        }
    }
    write_omitted(output, 4, values.omitted);
}

fn write_string_map(output: &mut String, label: &str, values: &InspectedStringMap) {
    if !values.entries.is_empty() {
        writeln!(output, "  {label}:").expect("writing to a string cannot fail");
        for (name, value) in &values.entries {
            writeln!(output, "    {}: {}", text_value(name), text_value(value))
                .expect("writing to a string cannot fail");
        }
    }
    write_omitted(output, 4, values.omitted);
}

fn write_omitted(output: &mut String, indentation: usize, omitted: usize) {
    if omitted > 0 {
        writeln!(output, "{}… {omitted} omitted", " ".repeat(indentation))
            .expect("writing to a string cannot fail");
    }
}

fn text_value(value: &str) -> std::borrow::Cow<'_, str> {
    if value.chars().any(char::is_control) {
        serde_json::to_string(value)
            .expect("strings always serialize")
            .into()
    } else {
        value.into()
    }
}

fn dependency_resolution_label(resolution: &DependencyResolution) -> &'static str {
    match resolution {
        DependencyResolution::Resolvable => "resolvable",
        DependencyResolution::InvalidName => "invalid-name",
        DependencyResolution::InvalidSpecification => "invalid-specification",
        DependencyResolution::UnsupportedHistoricName => "unsupported-historic-name",
        DependencyResolution::UnsupportedFile => "unsupported-file",
        DependencyResolution::UnsupportedGit => "unsupported-git",
        DependencyResolution::UnsupportedUrl => "unsupported-url",
    }
}

/// Resolve an NPM range against published versions in a local, parsed packument.
///
/// This deliberately mirrors production package elaboration: parse the range with
/// node-semver, convert packument version keys to node-semver versions, sort them,
/// then filter with the shared range elaborator. An empty result is successful and
/// means that no published version satisfies the range.
fn resolve_packument_versions(file: &Utf8Path, raw_range: &str) -> Result<Vec<RangeVersion>> {
    let range = parse_range(raw_range).context("invalid npm version range")?;
    let input =
        File::open(file).with_context(|| format!("failed to read packument file {file}"))?;
    let packument =
        parse_packument(input).with_context(|| format!("failed to parse packument file {file}"))?;
    let mut versions = packument
        .versions
        .keys()
        .map(|version| {
            RangeVersion::parse(version.to_string()).expect("packument versions are valid SemVer")
        })
        .collect::<Vec<_>>();
    versions.sort();

    Ok(elaborate_npm_version_bounds(&versions, &range)
        .expect("versions were sorted")
        .into_iter()
        .cloned()
        .collect())
}

fn refresh_corpus(registry: &Url, _token: DestructiveOperationToken) -> Result<()> {
    let catalog = load_catalog()?;
    let results = catalog
        .fixtures
        .iter()
        .map(|fixture| refresh_package(registry, &fixture.package))
        .collect::<Vec<_>>();
    let summary = CorpusRefreshSummary {
        schema_version: 1,
        checked_at: Utc::now().to_rfc3339(),
        registry: registry.as_str(),
        packages: results,
    };

    let state_path = corpus_refresh_state_path();
    let output = File::create(&state_path)
        .with_context(|| format!("failed to create corpus refresh state at {state_path}"))?;
    serde_json::to_writer_pretty(output, &summary)
        .context("failed to write corpus refresh summary")?;

    let accepted = summary
        .packages
        .iter()
        .filter(|package| package.status == CorpusStatus::Accepted)
        .count();
    println!(
        "packument corpus: {accepted}/{} accepted; state: {state_path}",
        summary.packages.len()
    );

    Ok(())
}

fn corpus_refresh_state_path() -> Utf8PathBuf {
    Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(CORPUS_REFRESH_STATE_FILE)
}

fn corpus_catalog_path() -> Utf8PathBuf {
    Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(CORPUS_CATALOG_FILE)
}

fn corpus_real_directory() -> Utf8PathBuf {
    Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(CORPUS_REAL_DIRECTORY)
}

fn corpus_cache_directory() -> Utf8PathBuf {
    Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(CORPUS_CACHE_DIRECTORY)
}

fn load_catalog() -> Result<CorpusCatalog> {
    let catalog_path = corpus_catalog_path();
    serde_json::from_reader(
        File::open(&catalog_path)
            .with_context(|| format!("failed to read corpus catalog at {catalog_path}"))?,
    )
    .context("failed to parse corpus catalog")
}

fn add_corpus_package(
    registry: &Url,
    package: &str,
    _token: DestructiveOperationToken,
) -> Result<()> {
    let registry_path = registry_path_for_package(package)?;
    let endpoint = registry
        .join(&registry_path)
        .with_context(|| format!("failed to build registry URL for {package}"))?;
    let body = fetch_packument(registry, package, &registry_path)?;
    let parsed = parse_packument(body.as_slice())
        .with_context(|| format!("registry response for {package} is not a supported packument"))?;

    if parsed.name.as_str() != package {
        bail!("registry response package name does not match requested package {package}");
    }

    let catalog_path = corpus_catalog_path();
    let mut catalog = load_catalog()?;
    if catalog
        .fixtures
        .iter()
        .any(|fixture| fixture.package == package)
    {
        bail!("corpus already contains package {package}");
    }

    let fixture = format!("{}.json", fixture_stem(package)?);
    let fixture_path = corpus_real_directory().join(&fixture);
    if fixture_path.exists() {
        bail!("corpus fixture already exists at {fixture_path}");
    }

    let cache_path = corpus_cache_directory().join(&fixture);
    fs::create_dir_all(cache_path.parent().expect("cache path has a parent"))
        .with_context(|| format!("failed to create corpus cache directory for {cache_path}"))?;
    fs::write(&cache_path, &body)
        .with_context(|| format!("failed to write untracked packument cache at {cache_path}"))?;
    fs::write(&fixture_path, minimize_packument_fixture(&body)?)
        .with_context(|| format!("failed to write corpus fixture at {fixture_path}"))?;
    catalog.fixtures.push(CorpusFixture {
        package: package.to_owned(),
        fixture: format!("real/{fixture}"),
        source: endpoint.to_string(),
        captured_at: Utc::now().date_naive().to_string(),
    });
    let catalog_file = File::create(&catalog_path)
        .with_context(|| format!("failed to update corpus catalog at {catalog_path}"))?;
    serde_json::to_writer_pretty(catalog_file, &catalog)
        .context("failed to write corpus catalog")?;

    println!("added packument corpus fixture: {package} ({fixture_path})");
    Ok(())
}

fn minimize_packument_fixture(body: &[u8]) -> Result<Vec<u8>> {
    let packument: serde_json::Value = serde_json::from_slice(body)
        .context("failed to parse fetched packument JSON for fixture minimization")?;
    let name = packument
        .get("name")
        .cloned()
        .context("fetched packument is missing name")?;
    let dist_tags = packument
        .get("dist-tags")
        .and_then(serde_json::Value::as_object)
        .context("fetched packument is missing dist-tags")?;
    let versions = packument
        .get("versions")
        .and_then(serde_json::Value::as_object)
        .context("fetched packument is missing versions")?;

    let mut selected_versions = serde_json::Map::new();
    for version in dist_tags.values() {
        let version = version
            .as_str()
            .context("packument dist tag does not name a version")?;
        let metadata = versions
            .get(version)
            .with_context(|| format!("packument dist tag points to missing version {version}"))?;
        selected_versions.insert(version.to_owned(), metadata.clone());
    }

    serde_json::to_vec_pretty(&serde_json::json!({
        "name": name,
        "dist-tags": dist_tags,
        "versions": selected_versions,
    }))
    .context("failed to serialize minimized packument fixture")
}

fn registry_path_for_package(package: &str) -> Result<String> {
    let stem = fixture_stem(package)?;
    if let Some(scoped) = package.strip_prefix('@') {
        let (scope, name) = scoped.split_once('/').expect("validated scoped package");
        return Ok(format!("%40{scope}%2F{name}"));
    }

    Ok(stem)
}

fn fixture_stem(package: &str) -> Result<String> {
    if package.is_empty() || package.len() > 214 {
        bail!("invalid npm package name {package:?}");
    }

    let scoped = package.starts_with('@');
    let package = package.strip_prefix('@').unwrap_or(package);
    let mut parts = package.split('/');
    let first = parts
        .next()
        .expect("nonempty package has a first component");
    let second = parts.next();
    if parts.next().is_some() || (scoped && second.is_none()) || (!scoped && second.is_some()) {
        bail!("invalid npm package name {package:?}");
    }

    let valid_component = |component: &str| {
        !component.is_empty()
            && !component.starts_with(['.', '_'])
            && component.bytes().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'-' | b'_' | b'.')
            })
    };
    if !valid_component(first) || second.is_some_and(|component| !valid_component(component)) {
        bail!("invalid npm package name {package:?}");
    }

    Ok(second.map_or_else(|| first.to_owned(), |second| format!("{first}--{second}")))
}

fn refresh_package(registry: &Url, package: &str) -> CorpusRefreshPackage {
    let registry_path = match registry_path_for_package(package) {
        Ok(path) => path,
        Err(error) => {
            return CorpusRefreshPackage {
                name: package.to_owned(),
                status: CorpusStatus::Rejected,
                versions: None,
                error: Some(error.to_string()),
            };
        }
    };

    match fetch_packument(registry, package, &registry_path) {
        Ok(body) => match parse_packument(body.as_slice()) {
            Ok(packument) => CorpusRefreshPackage {
                name: package.to_owned(),
                status: CorpusStatus::Accepted,
                versions: Some(packument.versions.len()),
                error: None,
            },
            Err(error) => CorpusRefreshPackage {
                name: package.to_owned(),
                status: CorpusStatus::Rejected,
                versions: None,
                error: Some(error.to_string()),
            },
        },
        Err(error) => CorpusRefreshPackage {
            name: package.to_owned(),
            status: CorpusStatus::FetchFailed,
            versions: None,
            error: Some(error.to_string()),
        },
    }
}

fn fetch_packument(registry: &Url, package: &str, registry_path: &str) -> Result<Vec<u8>> {
    let endpoint = registry
        .join(registry_path)
        .with_context(|| format!("failed to build registry URL for {package}"))?;
    let response = Command::new("curl")
        .args([
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--connect-timeout",
            "10",
            "--max-time",
            "30",
            "--max-filesize",
            MAX_PACKUMENT_BYTES,
            "--header",
            "Accept: application/json",
        ])
        .arg(endpoint.as_str())
        .output()
        .context("failed to run curl; install curl to refresh the packument corpus")?;

    if !response.status.success() {
        bail!(
            "curl failed for {}: {}",
            package,
            String::from_utf8_lossy(&response.stderr).trim()
        );
    }

    Ok(response.stdout)
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct CorpusCatalog {
    schema_version: u8,
    fixtures: Vec<CorpusFixture>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct CorpusFixture {
    package: String,
    fixture: String,
    source: String,
    captured_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CorpusRefreshSummary<'a> {
    schema_version: u8,
    checked_at: String,
    registry: &'a str,
    packages: Vec<CorpusRefreshPackage>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CorpusRefreshPackage {
    name: String,
    status: CorpusStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    versions: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum CorpusStatus {
    Accepted,
    Rejected,
    FetchFailed,
}

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_INSPECTION_LIMIT, MAX_INSPECTION_VALUE_CHARS, command, corpus_refresh_state_path,
        dependency_resolution_label, fixture_stem, format_inspection, inspect_packument,
        load_catalog, resolve_packument_versions,
    };
    use camino::Utf8PathBuf;
    use nv_common::npm::packument::parse_packument;

    const FULL_PACKUMENT: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../nv-common/testdata/npm/packument/full-packument.json"
    ));
    const ABBREVIATED_PACKUMENT: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../nv-common/testdata/npm/packument/abbreviated-packument.json"
    ));

    #[test]
    fn inspect_accepts_a_file_and_json_output() {
        command()
            .try_get_matches_from(["packument", "inspect", "fixture.json", "--json"])
            .expect("packument inspect should parse");
    }

    #[test]
    fn inspect_accepts_limit_options_and_rejects_invalid_combinations() {
        command()
            .try_get_matches_from(["packument", "inspect", "fixture.json", "--limit", "25"])
            .expect("packument inspect limit should parse");
        command()
            .try_get_matches_from(["packument", "inspect", "fixture.json", "--no-limit"])
            .expect("packument inspect no-limit should parse");
        command()
            .try_get_matches_from(["packument", "inspect", "fixture.json", "--limit", "0"])
            .expect_err("zero inspection limit should fail");
        command()
            .try_get_matches_from([
                "packument",
                "inspect",
                "fixture.json",
                "--limit",
                "25",
                "--no-limit",
            ])
            .expect_err("inspection limit options should conflict");
    }

    #[test]
    fn inspect_full_packument_prints_normalized_dependency_and_dist_metadata() {
        let packument = parse_packument(FULL_PACKUMENT.as_bytes()).expect("fixture should parse");

        assert_eq!(
            format_inspection(&inspect_packument(&packument, None)),
            concat!(
                "name: example\n",
                "dist-tags:\n",
                "  latest: 1.0.0\n",
                "available versions: 1\n",
                "version: 1.0.0\n",
                "  dependencies:\n",
                "    serde: ^1.0.0 (resolvable)\n",
                "  dev-dependencies:\n",
                "    serde_json: ^1.0.0 (resolvable)\n",
                "  peer-dependencies:\n",
                "    tokio: ^1.0.0 (resolvable)\n",
                "  optional-dependencies:\n",
                "    bytes: ^1.0.0 (resolvable)\n",
                "  tarball: https://registry.npmjs.org/example/-/example-1.0.0.tgz\n",
                "  shasum: 0123456789abcdef0123456789abcdef01234567\n",
                "  integrity: sha512-example\n",
                "  engines:\n",
                "    node: >=18\n",
            )
        );
    }

    #[test]
    fn inspect_abbreviated_packument_omits_unavailable_metadata() {
        let packument =
            parse_packument(ABBREVIATED_PACKUMENT.as_bytes()).expect("fixture should parse");

        assert_eq!(
            format_inspection(&inspect_packument(&packument, None)),
            concat!(
                "name: minimal-package\n",
                "dist-tags:\n",
                "  latest: 2.0.0\n",
                "available versions: 1\n",
                "version: 2.0.0\n",
                "  tarball: https://registry.npmjs.org/minimal-package/-/minimal-package-2.0.0.tgz\n",
                "  shasum: 0123456789abcdef0123456789abcdef01234567\n",
            )
        );
    }

    #[test]
    fn inspect_sorts_map_derived_output_and_versions() {
        let packument = parse_packument(
            &br#"{
                "name": "example",
                "dist-tags": { "zeta": "2.0.0", "alpha": "1.0.0" },
                "versions": {
                    "2.0.0": {
                        "name": "example", "version": "2.0.0",
                        "dependencies": { "zeta": "^2.0.0", "alpha": "^1.0.0" },
                        "dist": {
                            "tarball": "https://example.test/example-2.0.0.tgz",
                            "shasum": "0123456789abcdef0123456789abcdef01234567"
                        }
                    },
                    "1.0.0": {
                        "name": "example", "version": "1.0.0",
                        "dist": {
                            "tarball": "https://example.test/example-1.0.0.tgz",
                            "shasum": "0123456789abcdef0123456789abcdef01234567"
                        }
                    }
                }
            }"#[..],
        )
        .expect("fixture should parse");

        let output = format_inspection(&inspect_packument(&packument, None));
        assert!(output.contains("  alpha: 1.0.0\n  zeta: 2.0.0\n"));
        assert!(output.contains("version: 1.0.0\n"));
        assert!(output.contains("    alpha: ^1.0.0 (resolvable)\n    zeta: ^2.0.0 (resolvable)\n"));
        assert!(
            output.find("version: 1.0.0").expect("first version")
                < output.find("version: 2.0.0").expect("second version")
        );
    }

    #[test]
    fn inspect_limits_versions_and_entries() {
        let packument = parse_packument(
            &br#"{
                "name": "example",
                "dist-tags": { "latest": "2.0.0", "previous": "1.0.0" },
                "versions": {
                    "1.0.0": {
                        "name": "example", "version": "1.0.0",
                        "dependencies": { "alpha": "^1.0.0", "beta": "^1.0.0" },
                        "dist": { "tarball": "https://example.test/1.0.0.tgz", "shasum": "0123456789abcdef0123456789abcdef01234567" }
                    },
                    "2.0.0": {
                        "name": "example", "version": "2.0.0",
                        "dependencies": { "alpha": "^2.0.0", "beta": "^2.0.0" },
                        "dist": { "tarball": "https://example.test/2.0.0.tgz", "shasum": "0123456789abcdef0123456789abcdef01234567" }
                    }
                }
            }"#[..],
        )
        .expect("fixture should parse");

        let output = format_inspection(&inspect_packument(&packument, Some(1)));
        assert!(output.contains("available versions: 2\n… 1 omitted\nversion: 2.0.0\n"));
        assert!(output.contains("  latest: 2.0.0\n  … 1 omitted\n"));
        assert!(output.contains("    alpha: ^2.0.0 (resolvable)\n    … 1 omitted\n"));
        assert!(!output.contains("version: 1.0.0"));
    }

    #[test]
    fn inspect_default_limit_keeps_latest_versions() {
        let default_limit = usize::try_from(DEFAULT_INSPECTION_LIMIT)
            .expect("default inspection limit should fit in usize");
        let versions = (0..=DEFAULT_INSPECTION_LIMIT)
            .map(|patch| {
                let version = format!("1.0.{patch}");
                let metadata = serde_json::json!({
                    "name": "example",
                    "version": version,
                    "dist": {
                        "tarball": format!("https://example.test/{patch}.tgz"),
                        "shasum": "0123456789abcdef0123456789abcdef01234567"
                    }
                });
                (version, metadata)
            })
            .collect::<serde_json::Map<_, _>>();
        let fixture = serde_json::json!({
            "name": "example",
            "dist-tags": { "latest": format!("1.0.{DEFAULT_INSPECTION_LIMIT}") },
            "versions": versions,
        });
        let fixture = serde_json::to_vec(&fixture).expect("fixture should serialize");
        let packument = parse_packument(fixture.as_slice()).expect("fixture should parse");

        let inspection = inspect_packument(&packument, Some(default_limit));

        assert_eq!(inspection.available_versions, default_limit + 1);
        assert_eq!(inspection.omitted_versions, 1);
        assert_eq!(inspection.versions.len(), default_limit);
        assert_eq!(
            inspection
                .versions
                .first()
                .map(|version| version.version.as_str()),
            Some("1.0.1")
        );
        assert_eq!(
            inspection
                .versions
                .last()
                .map(|version| version.version.as_str()),
            Some("1.0.100")
        );
    }

    #[test]
    fn inspect_escapes_control_characters_and_marks_unusable_dependencies() {
        let packument = parse_packument(
            &br#"{
                "name": "example",
                "dist-tags": { "latest": "1.0.0" },
                "versions": {
                    "1.0.0": {
                        "name": "example", "version": "1.0.0",
                        "dependencies": { "bad\nname": "not a spec" },
                        "deprecated": "unsafe\u001b[2J",
                        "dist": { "tarball": "https://example.test/1.0.0.tgz", "shasum": "0123456789abcdef0123456789abcdef01234567" }
                    }
                }
            }"#[..],
        )
        .expect("fixture should parse");

        let output = format_inspection(&inspect_packument(&packument, None));
        assert!(output.contains("\"bad\\nname\": not a spec (invalid-name)"));
        assert!(output.contains("deprecated: \"unsafe\\u001b[2J\""));
        assert!(!output.contains("bad\nname: not a spec"));
    }

    #[test]
    fn inspect_truncates_oversized_external_values_in_text_and_json() {
        let oversized = "x".repeat(MAX_INSPECTION_VALUE_CHARS + 7);
        let fixture = serde_json::json!({
            "name": "example",
            "dist-tags": { "latest": "1.0.0" },
            "versions": {
                "1.0.0": {
                    "name": "example",
                    "version": "1.0.0",
                    "deprecated": oversized,
                    "dist": {
                        "tarball": "https://example.test/1.0.0.tgz",
                        "shasum": "0123456789abcdef0123456789abcdef01234567"
                    }
                }
            }
        });
        let fixture = serde_json::to_vec(&fixture).expect("fixture should serialize");
        let packument = parse_packument(fixture.as_slice()).expect("fixture should parse");
        let inspection = inspect_packument(&packument, None);
        let expected = format!(
            "{}… [7 characters omitted]",
            "x".repeat(MAX_INSPECTION_VALUE_CHARS)
        );

        assert_eq!(
            inspection.versions[0].deprecated.as_deref(),
            Some(expected.as_str())
        );
        assert!(format_inspection(&inspection).contains(&format!("deprecated: {expected}")));
        let json = serde_json::to_value(&inspection).expect("inspection should serialize");
        assert_eq!(json["versions"][0]["deprecated"], expected);
    }

    #[test]
    fn inspect_preserves_dist_tags_with_colliding_truncated_keys() {
        let shared_prefix = "x".repeat(MAX_INSPECTION_VALUE_CHARS);
        let first_tag = format!("{shared_prefix}1");
        let second_tag = format!("{shared_prefix}2");
        let rendered_tag = format!("{shared_prefix}… [1 characters omitted]");
        let fixture = serde_json::json!({
            "name": "example",
            "dist-tags": {
                first_tag: "1.0.0",
                second_tag: "2.0.0"
            },
            "versions": {
                "1.0.0": {
                    "name": "example",
                    "version": "1.0.0",
                    "dist": {
                        "tarball": "https://example.test/1.0.0.tgz",
                        "shasum": "0123456789abcdef0123456789abcdef01234567"
                    }
                },
                "2.0.0": {
                    "name": "example",
                    "version": "2.0.0",
                    "dist": {
                        "tarball": "https://example.test/2.0.0.tgz",
                        "shasum": "0123456789abcdef0123456789abcdef01234567"
                    }
                }
            }
        });
        let fixture = serde_json::to_vec(&fixture).expect("fixture should serialize");
        let packument = parse_packument(fixture.as_slice()).expect("fixture should parse");

        let inspection = inspect_packument(&packument, None);

        assert_eq!(inspection.dist_tags.omitted, 0);
        assert_eq!(inspection.dist_tags.entries.len(), 2);
        assert_eq!(
            inspection
                .dist_tags
                .entries
                .get(&rendered_tag)
                .map(String::as_str),
            Some("1.0.0")
        );
        assert_eq!(
            inspection
                .dist_tags
                .entries
                .get(&format!("{rendered_tag} [collision 2]"))
                .map(String::as_str),
            Some("2.0.0")
        );
    }

    #[test]
    fn inspect_counts_omitted_dependencies_before_truncated_key_collisions() {
        let shared_prefix = "y".repeat(MAX_INSPECTION_VALUE_CHARS);
        let first_name = format!("{shared_prefix}!1");
        let second_name = format!("{shared_prefix}!2");
        let rendered_name = format!("{shared_prefix}… [2 characters omitted]");
        let fixture = serde_json::json!({
            "name": "example",
            "dist-tags": { "latest": "1.0.0" },
            "versions": {
                "1.0.0": {
                    "name": "example",
                    "version": "1.0.0",
                    "dependencies": {
                        first_name: "^1.0.0",
                        second_name: "^2.0.0"
                    },
                    "dist": {
                        "tarball": "https://example.test/1.0.0.tgz",
                        "shasum": "0123456789abcdef0123456789abcdef01234567"
                    }
                }
            }
        });
        let fixture = serde_json::to_vec(&fixture).expect("fixture should serialize");
        let packument = parse_packument(fixture.as_slice()).expect("fixture should parse");

        let inspection = inspect_packument(&packument, Some(1));
        let dependencies = &inspection.versions[0].dependencies.dependencies;

        assert_eq!(dependencies.omitted, 1);
        assert_eq!(dependencies.entries.len(), 1);
        assert_eq!(
            dependencies.entries.get(&rendered_name).map(|dependency| {
                (
                    dependency.specification.as_str(),
                    dependency_resolution_label(&dependency.resolution),
                )
            }),
            Some(("^1.0.0", "invalid-name"))
        );
    }

    #[test]
    fn inspect_preserves_engines_with_colliding_truncated_keys() {
        let shared_prefix = "z".repeat(MAX_INSPECTION_VALUE_CHARS);
        let first_engine = format!("{shared_prefix}1");
        let second_engine = format!("{shared_prefix}2");
        let rendered_engine = format!("{shared_prefix}… [1 characters omitted]");
        let fixture = serde_json::json!({
            "name": "example",
            "dist-tags": { "latest": "1.0.0" },
            "versions": {
                "1.0.0": {
                    "name": "example",
                    "version": "1.0.0",
                    "engines": {
                        first_engine: ">=18",
                        second_engine: ">=20"
                    },
                    "dist": {
                        "tarball": "https://example.test/1.0.0.tgz",
                        "shasum": "0123456789abcdef0123456789abcdef01234567"
                    }
                }
            }
        });
        let fixture = serde_json::to_vec(&fixture).expect("fixture should serialize");
        let packument = parse_packument(fixture.as_slice()).expect("fixture should parse");

        let inspection = inspect_packument(&packument, None);
        let engines = &inspection.versions[0].engines;

        assert_eq!(engines.omitted, 0);
        assert_eq!(engines.entries.len(), 2);
        assert_eq!(
            engines.entries.get(&rendered_engine).map(String::as_str),
            Some(">=18")
        );
        assert_eq!(
            engines
                .entries
                .get(&format!("{rendered_engine} [collision 2]"))
                .map(String::as_str),
            Some(">=20")
        );
    }

    #[test]
    fn corpus_refresh_requires_destructive_flag() {
        let error = command()
            .try_get_matches_from(["packument", "corpus-refresh"])
            .expect_err("missing destructive flag should fail");

        assert_eq!(
            error.kind(),
            clap::error::ErrorKind::MissingRequiredArgument
        );
    }

    #[test]
    fn corpus_refresh_writes_the_fixed_project_state_file() {
        assert_eq!(
            corpus_refresh_state_path(),
            Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../nv-common/testdata/npm/packument/corpus-refresh.json")
        );
    }

    #[test]
    fn corpus_add_requires_destructive_flag() {
        let error = command()
            .try_get_matches_from(["packument", "corpus-add", "example"])
            .expect_err("missing corpus-add options should fail");

        assert_eq!(
            error.kind(),
            clap::error::ErrorKind::MissingRequiredArgument
        );
    }

    #[test]
    fn fixture_stems_preserve_scopes_without_path_separators() {
        assert_eq!(fixture_stem("example").unwrap(), "example");
        assert_eq!(fixture_stem("@scope/package").unwrap(), "scope--package");
        fixture_stem("../escape").expect_err("invalid package name should fail");
    }

    #[test]
    fn corpus_catalog_contains_diverse_public_packages() {
        let catalog = load_catalog().expect("corpus catalog should parse");
        let names = catalog
            .fixtures
            .iter()
            .map(|fixture| fixture.package.as_str())
            .collect::<Vec<_>>();

        assert!(names.contains(&"express"));
        assert!(names.contains(&"@babel/core"));
        assert!(names.contains(&"@types/node"));
    }

    #[test]
    fn resolve_uses_shared_caret_range_semantics_and_sorts_versions() {
        let fixture =
            Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/packument-resolve.json");

        let versions = resolve_packument_versions(&fixture, "^1.2.3")
            .expect("fixture and range should resolve")
            .into_iter()
            .map(|version| version.to_string())
            .collect::<Vec<_>>();

        assert_eq!(versions, ["1.2.3", "1.5.0"]);
    }

    #[test]
    fn resolve_rejects_invalid_ranges() {
        let fixture =
            Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/packument-resolve.json");

        let error = resolve_packument_versions(&fixture, "^1.2.3.4")
            .expect_err("invalid ranges must not resolve");

        assert!(error.to_string().contains("invalid npm version range"));
    }

    #[test]
    fn resolve_rejects_malformed_packuments() {
        let malformed_packument = Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../nv-common/testdata/npm/packument/corpus.json");

        let error = resolve_packument_versions(&malformed_packument, "^1.2.3")
            .expect_err("a corpus catalog is not a packument");

        assert!(error.to_string().contains("failed to parse packument file"));
    }

    #[test]
    fn resolve_succeeds_with_no_output_versions_when_range_has_no_matches() {
        let fixture =
            Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/packument-resolve.json");

        let versions = resolve_packument_versions(&fixture, "^3.0.0")
            .expect("a range without matches is still a successful resolution");

        assert!(versions.is_empty());
    }
}
