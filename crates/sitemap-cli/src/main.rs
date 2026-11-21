//! `advanced-sitemap` — command-line interface of the Advanced Sitemap engine.
//!
//! Exit codes (stable, documented in `docs/cli/README.md`):
//!
//! | Code | Meaning |
//! |---|---|
//! | 0 | Success |
//! | 1 | Generation failed or input invalid (or warnings with `--deny-warnings`) |
//! | 2 | Usage or configuration error |
//! | 3 | I/O error |
//! | 4 | Internal error |

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use advanced_sitemap::inspect::{FileInspection, inspect_file, inspect_reader};
use advanced_sitemap::plan::{Plan, explain};
use advanced_sitemap::protocol::{
    IndexSetting, InvalidPolicySetting, SourceFormat, SourceSection, StrategySetting,
    ValidationModeSetting,
};
use advanced_sitemap::{
    ConfigDocument, Diagnostic, Engine, EngineError, FsStore, GenerationReport, InspectOptions,
    NullStore, ReaderLimits, ResolvedConfig, RunMode, SeverityCounts, SitemapUrl, ValidationMode,
    ValidationOptions, W3cDateTime, limits, open_input, resolve_config,
};
use clap::{Args, Parser, Subcommand, ValueEnum};

const EXIT_FAILED: u8 = 1;
const EXIT_CONFIG: u8 = 2;
const EXIT_IO: u8 = 3;
const EXIT_INTERNAL: u8 = 4;

/// Universal sitemap engine: validate, plan, generate and inspect sitemaps.
#[derive(Parser)]
#[command(name = "advanced-sitemap", version, about, max_term_width = 100)]
struct Cli {
    /// Output format.
    #[arg(long, global = true, value_enum, default_value_t = Format::Human)]
    format: Format,

    #[command(subcommand)]
    command: Command,
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Format {
    Human,
    Json,
}

#[derive(Subcommand)]
enum Command {
    /// Validate the input and publish sitemap files.
    Generate {
        #[command(flatten)]
        common: CommonArgs,
        /// Output directory (overrides output.directory).
        #[arg(long, short)]
        out: Option<PathBuf>,
        /// Do not write the manifest file.
        #[arg(long)]
        no_manifest: bool,
        /// Exit with code 1 when warnings are reported.
        #[arg(long)]
        deny_warnings: bool,
    },
    /// Run the whole pipeline without writing anything.
    Validate {
        #[command(flatten)]
        common: CommonArgs,
        /// Exit with code 1 when warnings are reported.
        #[arg(long)]
        deny_warnings: bool,
    },
    /// Describe what `generate` would do, from a sample of the input. Writes nothing.
    Explain {
        #[command(flatten)]
        common: CommonArgs,
        /// Number of records to sample.
        #[arg(long, default_value_t = 1000)]
        sample: usize,
    },
    /// Read and check existing sitemap or sitemap index files (plain or gzip). Writes nothing.
    Inspect(InspectArgs),
}

#[derive(Args)]
struct InspectArgs {
    /// Local files to inspect; `-` reads standard input. Remote URLs are not supported yet.
    #[arg(required = true)]
    files: Vec<PathBuf>,
    /// Expected site origin, e.g. `https://example.com`.
    #[arg(long)]
    site: Option<String>,
    /// Reference time for time-dependent rules (W3C Datetime).
    #[arg(long)]
    reference_time: Option<String>,
    /// Use the strict validation profile for `<url>` entries.
    #[arg(long)]
    strict: bool,
    /// Do not report duplicate URLs.
    #[arg(long)]
    no_dedupe: bool,
    /// Maximum uncompressed bytes read per file (default: 52428800, the protocol limit).
    #[arg(long)]
    max_bytes: Option<u64>,
    /// Exit with code 1 when warnings are reported.
    #[arg(long)]
    deny_warnings: bool,
}

#[derive(Args)]
struct CommonArgs {
    /// Configuration document (protocol v1 JSON).
    #[arg(long, short)]
    config: Option<PathBuf>,
    /// Input file: .jsonl/.ndjson (streamed) or .json (array).
    #[arg(long, short)]
    input: Option<PathBuf>,
    /// Expected site origin, e.g. `https://example.com`.
    #[arg(long)]
    site: Option<String>,
    /// Public URL of the output directory (needed for an index).
    #[arg(long)]
    public_base_url: Option<String>,
    /// Reference time for time-dependent rules (W3C Datetime).
    #[arg(long)]
    reference_time: Option<String>,
    /// Use the strict validation profile.
    #[arg(long)]
    strict: bool,
    /// Policy for invalid records.
    #[arg(long, value_enum)]
    on_invalid: Option<OnInvalid>,
    /// Keep duplicate URLs instead of dropping them.
    #[arg(long)]
    no_dedupe: bool,
    /// Maximum diagnostics listed in the report.
    #[arg(long)]
    max_diagnostics: Option<u32>,
    /// Partitioning strategy.
    #[arg(long, value_enum)]
    strategy: Option<Strategy>,
    /// Partition count for the stable-hash strategy.
    #[arg(long)]
    partitions: Option<u32>,
    /// Maximum URLs per file (1-50000).
    #[arg(long)]
    max_urls: Option<u32>,
    /// Maximum bytes per file (4096-52428800).
    #[arg(long)]
    max_bytes: Option<u64>,
    /// Index policy.
    #[arg(long, value_enum)]
    index: Option<Index>,
    /// File stem (default: sitemap).
    #[arg(long)]
    file_stem: Option<String>,
}

#[derive(Clone, Copy, ValueEnum)]
enum OnInvalid {
    Fail,
    Skip,
}

#[derive(Clone, Copy, ValueEnum)]
enum Strategy {
    Sequential,
    StableHash,
}

#[derive(Clone, Copy, ValueEnum)]
enum Index {
    Auto,
    Always,
    Never,
}

struct Failure {
    code: u8,
    kind: &'static str,
    message: String,
}

impl From<EngineError> for Failure {
    fn from(error: EngineError) -> Self {
        let (code, kind) = match &error {
            EngineError::Config(_) | EngineError::Protocol(_) => (EXIT_CONFIG, "config"),
            EngineError::Io(_) => (EXIT_IO, "io"),
            EngineError::Xml(_) => (EXIT_INTERNAL, "internal"),
        };
        Failure {
            code,
            kind,
            message: error.to_string(),
        }
    }
}

fn config_failure(message: impl Into<String>) -> Failure {
    Failure {
        code: EXIT_CONFIG,
        kind: "config",
        message: message.into(),
    }
}

fn absolute(path: &Path) -> Result<PathBuf, Failure> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    std::env::current_dir()
        .map(|cwd| cwd.join(path))
        .map_err(|e| Failure {
            code: EXIT_IO,
            kind: "io",
            message: e.to_string(),
        })
}

/// Loads the configuration document (if any) and applies command-line overrides.
fn load(common: &CommonArgs, out: Option<&Path>) -> Result<ResolvedConfig, Failure> {
    let (mut doc, base_dir) = match &common.config {
        Some(path) => {
            let text = std::fs::read_to_string(path).map_err(|e| Failure {
                code: EXIT_IO,
                kind: "io",
                message: format!("{}: {e}", path.display()),
            })?;
            let doc = ConfigDocument::from_json(&text)
                .map_err(|e| config_failure(format!("{}: {e}", path.display())))?;
            let base = absolute(path)?
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_default();
            (doc, base)
        }
        None => (ConfigDocument::new(), absolute(Path::new("."))?),
    };

    if let Some(input) = &common.input {
        let path = absolute(input)?;
        let text = path.to_string_lossy().into_owned();
        let format = SourceFormat::from_path(&text).ok_or_else(|| {
            config_failure(format!(
                "cannot infer the format of {}: use a .jsonl, .ndjson or .json file",
                input.display()
            ))
        })?;
        doc.urls = None;
        doc.source = Some(SourceSection {
            format: Some(format),
            path: text,
            max_line_bytes: None,
        });
    }
    if let Some(site) = &common.site {
        doc.site = Some(site.clone());
    }
    if let Some(url) = &common.public_base_url {
        doc.public_base_url = Some(url.clone());
    }
    if let Some(time) = &common.reference_time {
        doc.reference_time = Some(time.clone());
    }
    if common.strict {
        doc.validation.mode = ValidationModeSetting::Strict;
    }
    if let Some(policy) = common.on_invalid {
        doc.validation.on_invalid = match policy {
            OnInvalid::Fail => InvalidPolicySetting::Fail,
            OnInvalid::Skip => InvalidPolicySetting::Skip,
        };
    }
    if common.no_dedupe {
        doc.validation.dedupe = false;
    }
    if let Some(max) = common.max_diagnostics {
        doc.validation.max_diagnostics = max;
    }
    if let Some(strategy) = common.strategy {
        doc.partitioning.strategy = match strategy {
            Strategy::Sequential => StrategySetting::Sequential,
            Strategy::StableHash => StrategySetting::StableHash,
        };
    }
    if common.partitions.is_some() {
        doc.partitioning.partitions = common.partitions;
    }
    if common.max_urls.is_some() {
        doc.partitioning.max_urls = common.max_urls;
    }
    if common.max_bytes.is_some() {
        doc.partitioning.max_bytes = common.max_bytes;
    }
    if let Some(index) = common.index {
        doc.output.index = match index {
            Index::Auto => IndexSetting::Auto,
            Index::Always => IndexSetting::Always,
            Index::Never => IndexSetting::Never,
        };
    }
    if let Some(stem) = &common.file_stem {
        doc.output.file_stem = stem.clone();
    }
    if let Some(out) = out {
        doc.output.directory = Some(absolute(out)?.to_string_lossy().into_owned());
    }

    let resolved = resolve_config(doc, &base_dir)?;
    if resolved.input.is_none() {
        return Err(config_failure(
            "no input: pass --input, or a --config with `urls` or `source`",
        ));
    }
    Ok(resolved)
}

fn run_engine(resolved: ResolvedConfig, mode: RunMode) -> Result<GenerationReport, Failure> {
    let input = resolved
        .input
        .as_ref()
        .ok_or_else(|| config_failure("no input"))?;
    let engine = Engine::new(resolved.engine.clone())?;
    let source = open_input(input)?;
    let report = match mode {
        RunMode::Publish => {
            let mut store = FsStore::new(&resolved.output_dir);
            engine.run(source, &mut store, mode)?
        }
        RunMode::DryRun => engine.run(source, &mut NullStore::new(), mode)?,
    };
    Ok(report)
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let format = cli.format;
    let result = match cli.command {
        Command::Generate {
            common,
            out,
            no_manifest,
            deny_warnings,
        } => load(&common, out.as_deref()).and_then(|mut resolved| {
            if no_manifest {
                resolved.engine.manifest = false;
            }
            let report = run_engine(resolved, RunMode::Publish)?;
            Ok(finish_report("generate", &report, format, deny_warnings))
        }),
        Command::Validate {
            common,
            deny_warnings,
        } => load(&common, None).and_then(|resolved| {
            let report = run_engine(resolved, RunMode::DryRun)?;
            Ok(finish_report("validate", &report, format, deny_warnings))
        }),
        Command::Explain { common, sample } => load(&common, None).and_then(|resolved| {
            let plan = explain(&resolved, sample.max(1))?;
            print_plan(&plan, format);
            Ok(ExitCode::SUCCESS)
        }),
        Command::Inspect(args) => run_inspect(&args, format),
    };

    match result {
        Ok(code) => code,
        Err(failure) => {
            match format {
                Format::Json => println!(
                    "{}",
                    serde_json::json!({
                        "schemaVersion": "1.0",
                        "status": "error",
                        "error": { "kind": failure.kind, "message": failure.message }
                    })
                ),
                Format::Human => eprintln!("advanced-sitemap: {}", failure.message),
            }
            ExitCode::from(failure.code)
        }
    }
}

fn inspect_options(args: &InspectArgs) -> Result<InspectOptions, Failure> {
    let site = args
        .site
        .as_deref()
        .map(|s| SitemapUrl::new(s).map_err(|e| config_failure(format!("--site: {e}"))))
        .transpose()?;
    let reference_time = args
        .reference_time
        .as_deref()
        .map(|s| {
            W3cDateTime::parse(s).map_err(|e| config_failure(format!("--reference-time: {e}")))
        })
        .transpose()?;
    let max_bytes = args.max_bytes.unwrap_or(limits::MAX_SITEMAP_BYTES);
    if max_bytes == 0 {
        return Err(config_failure("--max-bytes must be at least 1"));
    }
    Ok(InspectOptions {
        limits: ReaderLimits {
            max_bytes,
            ..ReaderLimits::default()
        },
        validation: ValidationOptions {
            mode: if args.strict {
                ValidationMode::Strict
            } else {
                ValidationMode::Standard
            },
            site,
            reference_time,
        },
        dedupe: !args.no_dedupe,
    })
}

fn run_inspect(args: &InspectArgs, format: Format) -> Result<ExitCode, Failure> {
    let options = inspect_options(args)?;
    let mut files = Vec::with_capacity(args.files.len());
    for path in &args.files {
        let io_failure = |e: std::io::Error| Failure {
            code: EXIT_IO,
            kind: "io",
            message: format!("{}: {e}", path.display()),
        };
        let inspection = if path.as_os_str() == "-" {
            inspect_reader("-", std::io::stdin().lock(), &options, |_| {}).map_err(io_failure)?
        } else {
            inspect_file(path, &options).map_err(io_failure)?
        };
        files.push(inspection);
    }
    let valid = files.iter().all(FileInspection::is_valid);
    let warned = files.iter().any(|f| f.severity_counts.warning > 0);
    match format {
        Format::Json => {
            let value = serde_json::json!({
                "schemaVersion": "1.0",
                "engineVersion": advanced_sitemap::ENGINE_VERSION,
                "command": "inspect",
                "status": if valid { "valid" } else { "invalid" },
                "files": files,
            });
            println!(
                "{}",
                serde_json::to_string_pretty(&value).unwrap_or_default()
            );
        }
        Format::Human => {
            for file in &files {
                print_inspection(file);
            }
        }
    }
    if !valid || (args.deny_warnings && warned) {
        Ok(ExitCode::from(EXIT_FAILED))
    } else {
        Ok(ExitCode::SUCCESS)
    }
}

fn print_inspection(file: &FileInspection) {
    println!("advanced-sitemap inspect {}", file.path);
    let status = match (file.complete, file.severity_counts.error) {
        (false, _) => "invalid (reading stopped)",
        (true, 0) => "valid",
        (true, _) => "invalid",
    };
    println!("  Status       {status}");
    println!(
        "  Document     {}{}, {} bytes uncompressed",
        file.kind.unwrap_or("unknown"),
        if file.compressed { " (gzip)" } else { "" },
        file.xml_bytes
    );
    println!(
        "  Entries      {} found, {} valid, {} duplicates",
        file.entries, file.valid_entries, file.duplicates
    );
    println!(
        "  Fields       {} with lastmod, {} with alternates, {} with images, {} with videos, {} with news",
        file.with_lastmod, file.with_alternates, file.with_images, file.with_videos, file.with_news
    );
    for (namespace, count) in &file.unsupported_extensions {
        println!("  Not checked  {count} elements of {namespace}");
    }
    print_diagnostics(&file.severity_counts, &file.diagnostics);
}

const HUMAN_DIAGNOSTIC_LIMIT: usize = 20;

fn print_diagnostics(counts: &SeverityCounts, diagnostics: &[Diagnostic]) {
    println!(
        "  Diagnostics  {} errors, {} warnings, {} recommendations, {} info",
        counts.error, counts.warning, counts.recommendation, counts.info
    );
    for diagnostic in diagnostics.iter().take(HUMAN_DIAGNOSTIC_LIMIT) {
        println!("    {diagnostic}");
        if let Some(value) = &diagnostic.value {
            println!("        value: {value}");
        }
        if let Some(suggestion) = &diagnostic.suggestion {
            println!("        fix:   {suggestion}");
        }
    }
    if diagnostics.len() > HUMAN_DIAGNOSTIC_LIMIT {
        println!(
            "    ... {} more listed in --format json",
            diagnostics.len() - HUMAN_DIAGNOSTIC_LIMIT
        );
    }
}

fn finish_report(
    command: &str,
    report: &GenerationReport,
    format: Format,
    deny_warnings: bool,
) -> ExitCode {
    match format {
        Format::Json => println!("{}", report.to_json()),
        Format::Human => print_report(command, report),
    }
    let warned = report.severity_counts.warning > 0 || report.severity_counts.error > 0;
    if !report.succeeded() || (deny_warnings && warned) {
        ExitCode::from(EXIT_FAILED)
    } else {
        ExitCode::SUCCESS
    }
}

fn print_report(command: &str, report: &GenerationReport) {
    let status = serde_json::to_value(report.status)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default();
    let s = &report.stats;
    println!("advanced-sitemap {} {command}", report.engine_version);
    println!("  Status       {status}");
    println!(
        "  Records      {} read, {} valid, {} invalid, {} duplicates",
        s.records, s.valid_records, s.invalid_records, s.duplicates
    );
    println!("  URLs         {} written", s.urls_written);
    println!(
        "  Store        {} ({})",
        report.store.kind, report.store.location
    );
    if !report.artifacts.is_empty() {
        let label = if command == "validate" {
            "Would write"
        } else {
            "Artifacts"
        };
        println!("  {label}");
        let width = report
            .artifacts
            .iter()
            .map(|a| a.name.len())
            .max()
            .unwrap_or(0);
        for artifact in &report.artifacts {
            let entries = artifact
                .entries
                .map(|n| format!("{n:>8} entries"))
                .unwrap_or_else(|| format!("{:>16}", ""));
            println!(
                "    {:<width$}  {entries}  {:>12} bytes",
                artifact.name, artifact.bytes
            );
        }
    }
    print_diagnostics(&report.severity_counts, &report.diagnostics);
}

fn print_plan(plan: &Plan, format: Format) {
    if format == Format::Json {
        println!("{}", serde_json::to_string_pretty(plan).unwrap_or_default());
        return;
    }
    let p = &plan.partitioning;
    println!("advanced-sitemap explain (nothing is written)");
    println!(
        "  Input source       {} ({})",
        plan.input.source, plan.input.reading
    );
    println!("  Input size         {} bytes", plan.input.bytes);
    println!("  Records            {}", plan.input.records);
    if let Some(sample) = &plan.input.sample {
        println!(
            "  Sample             {} records, {} valid, {} invalid, {} bytes per <url> on average",
            sample.records, sample.valid, sample.invalid, sample.average_url_bytes
        );
    }
    println!(
        "  Validation         {} (on invalid: {})",
        plan.validation, plan.on_invalid
    );
    println!(
        "  Deduplication      {}{}",
        if plan.dedupe { "on" } else { "off" },
        if plan.dedupe {
            format!(", memory {} bytes", plan.dedupe_memory_bytes)
        } else {
            String::new()
        }
    );
    println!(
        "  Strategy           streaming writer, {} partitioning",
        p.strategy
    );
    println!(
        "  Limits per file    {} URLs, {} bytes",
        p.max_urls, p.max_bytes
    );
    println!("  Sitemap files      {}", p.files);
    if let Some(n) = p.suggested_partitions {
        println!("  Suggested parts    {n} (stable-hash, 20% headroom)");
    }
    println!("  Output layout      {}", plan.output_layout);
    println!("  Output directory   {}", plan.output_directory);
    for note in &plan.notes {
        println!("  Note               {note}");
    }
}
