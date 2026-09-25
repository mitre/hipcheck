use crate::workspace::get_workspace_path;
use anyhow::{Context as _, Result, anyhow};
use clap::ArgMatches;
use itertools::Itertools as _;
use newtype_uuid::{Timestamp, TypedUuid, TypedUuidKind, TypedUuidTag};
use patharg::InputArg;
use pathbuf::pathbuf;
use petgraph::{
    Graph,
    dot::{Config, Dot},
    graph::NodeIndex,
};
use serde::Deserialize;
use serde_json::Value as JsonValue;
use std::{
    fmt::Write as _,
    fs::create_dir_all,
    iter,
    path::{Path, PathBuf},
};
use uuid::ContextV7;
use xshell::{Shell, cmd};

/// Produce a unit graph from Cargo's output and output its visualizations.
pub fn unit_graph(args: &ArgMatches) -> Result<()> {
    let input = args
        .get_one::<PathBuf>("input")
        // Technically the field is marked "required", so we *could* unwrap here, but having an
        // explicit error case is more defensive against future changes.
        .ok_or_else(|| anyhow!("missing input (stdin or file)"))?;

    let input = InputArg::from_arg(input);

    let ws = Workspace::new()?;

    println!("Starting build:");
    println!("\tBuild ID: {}", ws.build_id);

    let json = ws
        .get_unit_graph(&input)
        .context("failed to parse unit graph JSON from input")?;

    let json_path = ws
        .write_json(&json)
        .context("failed to write unit graph JSON to disk")?;

    let graph = ws
        .process_unit_graph(json)
        .context("failed to construct graph from JSON")?;

    let dot_path = ws
        .write_dot(&graph)
        .context("failed to write .dot file to disk")?;

    let svg_path = ws
        .write_svg(&dot_path)
        .context("failed to generate .svg file from .dot file")?;

    println!();
    println!("Unit graph statistics:");
    println!("\tNumber of nodes: {}", graph.node_count());
    println!("\tNumber of edges: {}", graph.edge_count());
    println!();
    println!("Wrote files:");
    println!("\tJSON: {}", json_path.display());
    println!("\tDOT:  {}", dot_path.display());
    println!("\tSVG:  {}", svg_path.display());

    Ok(())
}

/// Wrapper for interacting with the workspace.
struct Workspace {
    /// Handle to the shell, needed for running external commands.
    shell: Shell,

    /// The root directory of the workspace.
    workspace_root_dir: PathBuf,

    /// The "graph" directory, under `target/`, where we'll write output.
    graph_dir: PathBuf,

    /// The "build ID", a UUIDv7 that uniquely identifies the build.
    build_id: TypedUuid<UnitGraphTag>,
}

impl Workspace {
    /// Initialize a new workspace.
    ///
    /// This creates the `graph_dir` if it doesn't already exist. From here on, we assume it's still
    /// there. If for some reason the `graph_dir` is deleted mid-execution, later steps will fail.
    fn new() -> Result<Self> {
        let shell = Shell::new()?;
        let workspace_root_dir = get_workspace_path()?;
        let graph_dir = pathbuf![&workspace_root_dir, "target", "graph"];

        // We use v7 UUIDs to ensure the resulting files in the `graph_dir` are time-sorted, to
        // make it easier for users to find the "latest" unit graph when debugging build
        // performance issues.
        let build_id: TypedUuid<UnitGraphTag> = TypedUuid::new_v7(Timestamp::now(ContextV7::new()));

        // Create the graph directory if it doesn't already exist.
        create_dir_all(&graph_dir).with_context(|| {
            anyhow!(
                "failed to create graph output directory '{}'",
                graph_dir.display()
            )
        })?;

        Ok(Self {
            shell,
            workspace_root_dir,
            graph_dir,
            build_id,
        })
    }

    /// Get the unit graph from the input (`stdin` or a named file).
    ///
    /// This succeeds so long as the input is valid JSON.
    fn get_unit_graph(&self, input: &InputArg) -> Result<JsonValue> {
        // This exploits the fact that `InputArg` abstracts over reading from `stdin` vs. reading
        // from a file. Pretty sweet!
        let string = input.read_to_string()?;
        let value = serde_json::from_str(&string)?;
        Ok(value)
    }

    /// Generate a "real" graph from the graph represented in the JSON.
    ///
    /// This validates that the JSON we parsed earlier is actually in the expected format, and
    /// then constructs a `petgraph` `Graph` which enables us to use real graph algorithms after.
    fn process_unit_graph(&self, json: JsonValue) -> Result<UnitGraph> {
        let json: RawUnitGraph = serde_json::from_value(json)?;

        // We don't track any edge data because there's only one "type" of edge, indicating a
        // build dependency (meaning that a particular codegen unit relies on another unit
        // completing its build before the first unit can be built).
        let mut graph = Graph::new();

        // IMPORTANT NOTE: When constructing this graph, notice we're *only* using the indices
        // pulled from the source JSON data, but we're using them as implicit indices into the
        // graph itself (later, when we do edge construction). This only works for `petgraph`'s
        // default `Graph` type because that graph uses an underlying "adjacency list"
        // representation, implemented as two vectors: one for nodes, one for edges. Since we're
        // inserting into the node vector in the same order found in the source JSON array, we're
        // guaranteed to get the same index values between the two (also since both the JSON source
        // and Rust use 0-indexed arrays).
        //
        // If we changed graph representation in the future, this invariant might not hold, and
        // we'd need to instead maintain a mapping from "JSON-indices" (the indices used in the
        // source JSON file) and "petgraph-indices" (the indices used in the petgraph graph
        // data structure).
        for unit in json.units {
            graph.add_node(unit);
        }

        // We do this trick with a separate `edges` vector followed by `graph.extend_with_edges`
        // because otherwise we'd have overlapping immutable and mutable borrows when we try to
        // extend the graph within the for-loop while holding a reference to the graph with
        // `node_indices`. Rust smartly saves us from any accidental iterator invalidation.
        let mut edges: Vec<(NodeIndex, NodeIndex)> = Vec::new();
        for node_idx in graph.node_indices() {
            let node = &graph[node_idx];

            if node.dependencies.is_empty() {
                continue;
            }

            // Note the neat iterator trick here! We're `zip`-ing an infinite-length iterator
            // (`iter::repeat`, though it's not actually infinite, since Rust iterators are lazy)
            // with a finite-length iterator over the current node's dependencies. The way
            // `Iterator::zip` works, this will go until the end of the shorter iterator, meaning
            // we end up with an iterator that produces the same number of elements as the node
            // dependencies iterator it's wrapping.
            let deg_edges: Vec<(NodeIndex, NodeIndex)> = Iterator::zip(
                iter::repeat(node_idx),
                node.dependencies
                    .iter()
                    .map(|dep| NodeIndex::new(dep.index)),
            )
            .collect();

            edges.extend(&deg_edges[..]);
        }

        // Unfortunately, this function requires our node type to implement the `Default` trait,
        // because this call would generate a default node for any edge indices referencing a node
        // that doesn't already exist. That default-generation never happens in our code (we only
        // fill in edges after we've completely filled in nodes), but the trait bound is there
        // anyway. If `petgraph` ever offers an equivalent without a `Default` bound (something
        // like `try_extend_with_edges`), we should use it instead.
        graph.extend_with_edges(&edges[..]);

        Ok(graph)
    }

    /// Generate a JSON file for the `json` value.
    ///
    /// We use the "pretty" representation to make sure the JSON is actually human-readable, since
    /// these files are intended to support debugging by humans, which may include manual review of
    /// the unit graph JSON file.
    fn write_json(&self, json: &JsonValue) -> Result<PathBuf> {
        let pretty = serde_json::to_string_pretty(json)?;
        let json_path = self.write_graph_file("json", &pretty)?;
        Ok(json_path)
    }

    /// Prepare a Dot handle to the graph, for pretty-printing purposes.
    ///
    /// While we take in a "real" graph with full build unit data, we convert each node to a
    /// "pretty-printed" version intended to be more readable. Currently, this throws away
    /// information that we probably want to expose.
    ///
    /// In the future we could also likely want to modify the .dot file to change things like
    /// output shapes.
    fn write_dot(&self, graph: &UnitGraph) -> Result<PathBuf> {
        let graph = graph.map(
            |idx, node| self.pretty_print_node(idx, node),
            |_, ()| String::new(),
        );

        let dot = Dot::with_attr_getters(
            &graph,
            &[Config::EdgeNoLabel, Config::NodeNoLabel],
            &|_, _| String::new(),
            &|_, (_, node)| {
                // This configuration is intended to make individual nodes a bit more readable.
                // By default, GraphViz will use oval node shapes, no margin, and a serif font.
                // This overrides all of that, giving us rectangles with a small margin, and a
                // basic monospace font (whatever your system default is).
                format!("shape=box, margin=0.3, fontname=\"monospace\", label=\"{node}\"")
            },
        );

        let dot_path = self.write_graph_file("dot", &dot.to_string())?;

        Ok(dot_path)
    }

    /// Generate an SVG based on the .dot file at the given path.
    ///
    /// We use the defauly layout algorithm offered by `dot` (GraphViz`), which isn't particularly
    /// readable but is the *most readable* option compared to the other layouts offered. The
    /// reality is that the structure of the unit graph is pretty complicated, and the resulting
    /// graph is fairly dense, meaning none of the layout algorithms GraphViz offers are able to
    /// make it make much visual sense.
    fn write_svg(&self, dot_path: &Path) -> Result<PathBuf> {
        // Use the same filename as the .dot file, just replace the extension with "svg".
        let svg_path = dot_path.to_owned().with_extension("svg");

        // We assume the user has the `dot` CLI since GraphViz (which provides it) is part of our
        // Flox environment.
        cmd!(self.shell, "dot -Tsvg {dot_path} -o {svg_path}")
            .quiet()
            .run()?;

        Ok(svg_path)
    }

    /// Write out a file to the `target/graph/` directory with a `build_id` prefix.
    ///
    /// We include the build ID as a prefix for all files to make them easy to navigate in the
    /// target graph folder, and to ensure that the output for each run of the command is retained.
    ///
    /// We use v7 UUIDs to ensure time ordering, to help with navigation of the resulting folder.
    fn write_graph_file(&self, ext: &str, content: &str) -> Result<PathBuf> {
        let path = pathbuf![
            &self.graph_dir,
            &format!("{}-unit-graph.{}", self.build_id, ext)
        ];

        std::fs::write(&path, content)?;

        Ok(path)
    }

    /// Build a pretty string representation of build unit data.
    fn pretty_print_node(&self, idx: NodeIndex, node: &BuildUnit) -> String {
        macro_rules! write_field {
            ($sink:expr, $field_name:literal, $field_value:expr) => {
                // Writing to an in-memory `String` can never fail, but the `writeln!` macro
                // doesn't know that at compile time, and will always return `Result`. We're
                // unwrapping here, but none of these writes will ever fail (unless we're out of
                // memory to allocate, I suppose, but then we have much worse problems).
                //
                // Note the `\\l`, which is a directive in the DOT language indicating that each
                // line should be left-aligned. Without it, we'd get awkward center-aligned lines.
                //
                // Note that the "25" here is the number of characters of the longest field we're
                // printing. If longer fields are ever added, increment the number appropriately.
                write!($sink, "{:<25} {}\\l", $field_name, $field_value).unwrap()
            };
        }

        let mut out = String::new();

        // We include this extra "index" field to make navigating the visualization easier.
        // Sometimes the edges are hard to follow visually, so in those cases you can search
        // "node:<idx>" from a node's "dependencies" field to find the specific node you need.
        write_field!(&mut out, "index", format!("node:{}", idx.index()));
        write_field!(&mut out, "pkg_id", {
            let crates_io_prefix = "registry+https://github.com/rust-lang/crates.io-index#";
            let local_prefix = format!("path+file://{}/", self.workspace_root_dir.display());

            let pkg_id = node.pkg_id.replace(crates_io_prefix, "crates.io:");

            pkg_id.replace(&local_prefix, "local:")
        });
        write_field!(
            &mut out,
            "platform",
            node.platform.as_deref().unwrap_or("(current)")
        );
        write_field!(&mut out, "mode", node.mode);
        write_field!(&mut out, "features", {
            if node.features.is_empty() {
                String::from("(none)")
            } else {
                node.features.join(", ")
            }
        });
        write_field!(&mut out, "is_std", node.is_std.unwrap_or(false));
        write_field!(&mut out, "target.kind", node.target.kind.join(", "));
        write_field!(
            &mut out,
            "target.crate_types",
            node.target.crate_types.join(", ")
        );
        write_field!(&mut out, "target.name", node.target.name);
        write_field!(&mut out, "target.src_path", {
            match std::env::home_dir() {
                None => node.target.src_path.clone(),
                Some(home) => {
                    let src_path = Path::new(&node.target.src_path);
                    match src_path.strip_prefix(home) {
                        Ok(stripped) => stripped.display().to_string(),
                        Err(_) => node.target.src_path.clone(),
                    }
                }
            }
        });
        write_field!(&mut out, "target.edition", node.target.edition);
        write_field!(&mut out, "target.test", node.target.test);
        write_field!(&mut out, "target.doctest", node.target.doctest);
        write_field!(&mut out, "profile.name", node.profile.name);
        write_field!(&mut out, "profile.opt_level", node.profile.opt_level);
        write_field!(
            &mut out,
            "profile.root",
            node.profile.root.as_deref().unwrap_or("(none)")
        );
        write_field!(&mut out, "profile.lto", node.profile.lto);
        write_field!(
            &mut out,
            "profile.codegen_units",
            node.profile
                .codegen_units
                .map(|c| c.to_string())
                .as_deref()
                .unwrap_or("(rustc default)")
        );
        write_field!(&mut out, "profile.debuginfo", node.profile.debuginfo);
        write_field!(
            &mut out,
            "profile.debug_assertions",
            node.profile.debug_assertions
        );
        write_field!(
            &mut out,
            "profile.overflow_checks",
            node.profile.overflow_checks
        );
        write_field!(&mut out, "profile.rpath", node.profile.rpath);
        write_field!(&mut out, "profile.incremental", node.profile.incremental);
        write_field!(&mut out, "profile.panic", node.profile.panic);
        write_field!(
            &mut out,
            "dependencies",
            node.dependencies
                .iter()
                .map(|d| format!("node:{}", d.index))
                .join(", ")
        );

        out
    }
}

/// Empty type to tag the UUID for identifying graph builds.
///
/// We're using the `newtype-uuid` crate from Rain Paharia to handle UUIDs in our code. This crate
/// basically wraps the "normal" UUID crate, but helps ensure within our program that you never
/// confuse different "types" of UUIDs. This is basically an instance of defensive programming,
/// where if you have programs that may generate UUIDs for many different objects, you really want
/// to avoid stuffing the wrong UUIDs into the wrong places.
///
/// It works by having us define an empty type (in our case, an empty enum), implementing the
/// `TypedUuidKind` trait for it, which describes a "tag" we associate with our UUIDs for
/// debugging purposes (the tag is never written out if we use the UUID normally).
enum UnitGraphTag {}

impl TypedUuidKind for UnitGraphTag {
    fn tag() -> newtype_uuid::TypedUuidTag {
        const TAG: TypedUuidTag = TypedUuidTag::new("unit_graph");
        TAG
    }
}

/// Graph representing individual build units and how they relate to each other.
///
/// This is the "real" unit graph, constructed from the `RawUnitGraph`.
type UnitGraph = Graph<BuildUnit, ()>;

/// Representation of the deserialized unit graph.
///
/// This mirrors the top-level struct used in Cargo: `SerializedUnitGraph`.
///
/// Where original structures in the Cargo output get serialized to plain strings, we just
/// deserialize them as strings here, since our only goal is to output them anyway, not to take
/// action based on their implied structure.
///
/// See: <https://doc.rust-lang.org/cargo/reference/unstable.html#unit-graph>
/// See: <https://github.com/rust-lang/cargo/blob/master/src/cargo/core/compiler/unit_graph.rs#L56>
#[derive(Deserialize)]
struct RawUnitGraph {
    units: Vec<BuildUnit>,
}

/// A single compilation unit in the unit graph.
#[derive(Deserialize, Default)]
struct BuildUnit {
    pkg_id: String,
    target: Target,
    profile: Profile,
    platform: Option<String>,
    mode: String,
    features: Vec<String>,
    is_std: Option<bool>,
    dependencies: Vec<BuildUnitDep>,
}

/// Refers to another `BuildUnit` that is a dependency of the current `BuildUnit`.
#[derive(Deserialize)]
struct BuildUnitDep {
    index: usize,
}

/// Specifies the "target" being build, including what kind of crates it is and other metadata.
#[derive(Deserialize, Default)]
struct Target {
    kind: Vec<String>,
    crate_types: Vec<String>,
    name: String,
    src_path: String,
    edition: String,
    test: bool,
    doctest: bool,
}

/// Describes the build profile ("dev", "release", "test", etc.) and its configuration.
#[derive(Deserialize, Default)]
struct Profile {
    name: String,
    opt_level: String,
    root: Option<String>,
    lto: String,
    codegen_units: Option<u32>,
    debuginfo: u32,
    debug_assertions: bool,
    overflow_checks: bool,
    rpath: bool,
    incremental: bool,
    panic: String,
}
