use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use memlink_evaluator::{JsonlEvaluator, comparison_report, summarize_events};
use memlink_memory::{MemoryQuery, MemoryStore, SqliteMemoryStore};
use memlink_observe::{capture_process_snapshot, write_snapshot};
use memlink_protocol::{ExperimentId, RunMode};
use memlink_runtime::{BenchSuite, Runtime, TaskSpec};
use memlink_state::{InMemoryStateStore, MmapFileStateStore, StateStore, deterministic_embedding};
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Parser)]
#[command(
    name = "memlink",
    version,
    about = "MemLink multi-agent communication and memory MVP"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Run {
        #[arg(long, value_enum)]
        mode: CliMode,
        #[arg(long)]
        task_file: PathBuf,
        #[arg(long, default_value = "data/memlink.sqlite")]
        memory_db: PathBuf,
        #[arg(long, default_value = "reports/events.jsonl")]
        events: PathBuf,
        #[arg(long, value_enum, default_value_t = StateBackend::InMemory)]
        state_backend: StateBackend,
        #[arg(long, default_value = "data/state")]
        state_dir: PathBuf,
    },
    Bench {
        #[arg(long)]
        suite: PathBuf,
        #[arg(long, default_value_t = 10)]
        rounds: usize,
        #[arg(long, default_value = "data/memlink.sqlite")]
        memory_db: PathBuf,
        #[arg(long, default_value = "reports")]
        output_dir: PathBuf,
        #[arg(long, value_enum, default_value_t = StateBackend::InMemory)]
        state_backend: StateBackend,
        #[arg(long, default_value = "data/state")]
        state_dir: PathBuf,
    },
    Memory {
        #[command(subcommand)]
        command: MemoryCommand,
    },
    Report {
        #[arg(long)]
        text_events: PathBuf,
        #[arg(long)]
        structured_events: PathBuf,
        #[arg(long, default_value = "reports/report.md")]
        output: PathBuf,
    },
    Observe {
        #[arg(long, default_value = "reports/observe.json")]
        output: PathBuf,
        #[arg(long, default_value = "manual snapshot")]
        note: String,
    },
    Demo {
        #[arg(long, default_value = "reports/demo")]
        output_dir: PathBuf,
        #[arg(long, default_value_t = 10)]
        rounds: usize,
    },
    Audit {
        #[arg(long, default_value = "reports/demo")]
        input_dir: PathBuf,
        #[arg(long, default_value_t = 10)]
        min_tasks: u64,
        #[arg(long, default_value_t = 1)]
        min_state_files: usize,
        #[arg(long, default_value_t = 1)]
        min_memory_hits: usize,
        #[arg(long, default_value_t = 0.0, allow_hyphen_values = true)]
        min_text_saving: f64,
        #[arg(long, default_value_t = 0.0, allow_hyphen_values = true)]
        min_byte_saving: f64,
    },
}

#[derive(Debug, Subcommand)]
enum MemoryCommand {
    Search {
        #[arg(long)]
        query: String,
        #[arg(long, value_delimiter = ',')]
        tags: Vec<String>,
        #[arg(long, default_value_t = 5)]
        limit: usize,
        #[arg(long, default_value = "data/memlink.sqlite")]
        memory_db: PathBuf,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliMode {
    Text,
    Structured,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum StateBackend {
    InMemory,
    MmapFile,
}

impl From<CliMode> for RunMode {
    fn from(value: CliMode) -> Self {
        match value {
            CliMode::Text => Self::Text,
            CliMode::Structured => Self::Structured,
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Run {
            mode,
            task_file,
            memory_db,
            events,
            state_backend,
            state_dir,
        } => {
            let task = read_task(&task_file).await?;
            let outcome = run_one(
                mode.into(),
                task,
                memory_db,
                events,
                Uuid::new_v4(),
                state_backend,
                state_dir,
            )
            .await?;
            println!("{}", serde_json::to_string_pretty(&outcome)?);
        }
        Command::Bench {
            suite,
            rounds,
            memory_db,
            output_dir,
            state_backend,
            state_dir,
        } => {
            tokio::fs::create_dir_all(&output_dir).await?;
            let text_events = output_dir.join("text-events.jsonl");
            let structured_events = output_dir.join("structured-events.jsonl");
            let text_memory_db = mode_memory_path(&memory_db, "text");
            let structured_memory_db = mode_memory_path(&memory_db, "structured");
            let report = output_dir.join("report.md");
            let _ = tokio::fs::remove_file(&text_events).await;
            let _ = tokio::fs::remove_file(&structured_events).await;
            let _ = tokio::fs::remove_file(&text_memory_db).await;
            let _ = tokio::fs::remove_file(&structured_memory_db).await;
            let _ = tokio::fs::remove_dir_all(state_dir.join("text")).await;
            let _ = tokio::fs::remove_dir_all(state_dir.join("structured")).await;
            let suite = read_suite(&suite).await?;
            let experiment_id = Uuid::new_v4();
            for index in 0..rounds {
                let task = suite.tasks[index % suite.tasks.len()].clone();
                if let Err(error) = run_one(
                    RunMode::Text,
                    task.clone(),
                    text_memory_db.clone(),
                    text_events.clone(),
                    experiment_id,
                    state_backend,
                    state_dir.join("text"),
                )
                .await
                {
                    eprintln!("text round {} failed: {error:#}", index + 1);
                }
                if let Err(error) = run_one(
                    RunMode::Structured,
                    task,
                    structured_memory_db.clone(),
                    structured_events.clone(),
                    experiment_id,
                    state_backend,
                    state_dir.join("structured"),
                )
                .await
                {
                    eprintln!("structured round {} failed: {error:#}", index + 1);
                }
            }
            let text_summary = summarize_events(&text_events, Some(RunMode::Text)).await?;
            let structured_summary =
                summarize_events(&structured_events, Some(RunMode::Structured)).await?;
            let markdown = comparison_report(&text_summary, &structured_summary);
            tokio::fs::write(&report, markdown).await?;
            println!("report={}", report.display());
            println!("text_events={}", text_events.display());
            println!("structured_events={}", structured_events.display());
        }
        Command::Memory { command } => match command {
            MemoryCommand::Search {
                query,
                tags,
                limit,
                memory_db,
            } => {
                let store = SqliteMemoryStore::open(memory_db)?;
                let hits = store
                    .search(MemoryQuery {
                        query: query.clone(),
                        tags,
                        embedding: deterministic_embedding(&query, 32),
                        limit,
                    })
                    .await?;
                println!("{}", serde_json::to_string_pretty(&hits)?);
            }
        },
        Command::Report {
            text_events,
            structured_events,
            output,
        } => {
            let text_summary = summarize_events(text_events, Some(RunMode::Text)).await?;
            let structured_summary =
                summarize_events(structured_events, Some(RunMode::Structured)).await?;
            if let Some(parent) = output.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            tokio::fs::write(
                &output,
                comparison_report(&text_summary, &structured_summary),
            )
            .await?;
            println!("report={}", output.display());
        }
        Command::Observe { output, note } => {
            let snapshot = capture_process_snapshot(note).await?;
            write_snapshot(&output, &snapshot).await?;
            println!("{}", serde_json::to_string_pretty(&snapshot)?);
        }
        Command::Demo { output_dir, rounds } => {
            run_demo(output_dir, rounds).await?;
        }
        Command::Audit {
            input_dir,
            min_tasks,
            min_state_files,
            min_memory_hits,
            min_text_saving,
            min_byte_saving,
        } => {
            let report = audit_demo(
                input_dir,
                min_tasks,
                min_state_files,
                min_memory_hits,
                min_text_saving,
                min_byte_saving,
            )
            .await?;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
    }
    Ok(())
}

#[derive(Debug, serde::Serialize)]
struct AuditReport {
    passed: bool,
    checks: Vec<AuditCheck>,
}

#[derive(Debug, serde::Serialize)]
struct AuditCheck {
    name: String,
    passed: bool,
    detail: String,
}

impl AuditReport {
    fn new(checks: Vec<AuditCheck>) -> Self {
        let passed = checks.iter().all(|check| check.passed);
        Self { passed, checks }
    }

    fn ensure_passed(&self) -> Result<()> {
        if self.passed {
            Ok(())
        } else {
            let failed = self
                .checks
                .iter()
                .filter(|check| !check.passed)
                .map(|check| format!("{}: {}", check.name, check.detail))
                .collect::<Vec<_>>()
                .join("; ");
            anyhow::bail!("audit failed: {failed}")
        }
    }
}

fn check(name: impl Into<String>, passed: bool, detail: impl Into<String>) -> AuditCheck {
    AuditCheck {
        name: name.into(),
        passed,
        detail: detail.into(),
    }
}

async fn audit_demo(
    input_dir: PathBuf,
    min_tasks: u64,
    min_state_files: usize,
    min_memory_hits: usize,
    min_text_saving: f64,
    min_byte_saving: f64,
) -> Result<AuditReport> {
    let text_events = input_dir.join("text-events.jsonl");
    let structured_events = input_dir.join("structured-events.jsonl");
    let report_md = input_dir.join("report.md");
    let observe = input_dir.join("observe.json");
    let memory_search = input_dir.join("memory-search.json");
    let state_dir = input_dir.join("state");
    let text_summary = summarize_events(&text_events, Some(RunMode::Text)).await?;
    let structured_summary =
        summarize_events(&structured_events, Some(RunMode::Structured)).await?;
    let state_file_count = count_files(state_dir).await?;
    let memory_hit_count = read_memory_hit_count(&memory_search).await?;
    let text_saving =
        memlink_evaluator::saving(structured_summary.text_chars, text_summary.text_chars);
    let byte_saving =
        memlink_evaluator::saving(structured_summary.encoded_bytes, text_summary.encoded_bytes);
    let checks = vec![
        check(
            "text_tasks",
            text_summary.task_count >= min_tasks
                && text_summary.success_count == text_summary.task_count,
            format!(
                "tasks={}, success={}",
                text_summary.task_count, text_summary.success_count
            ),
        ),
        check(
            "structured_tasks",
            structured_summary.task_count >= min_tasks
                && structured_summary.success_count == structured_summary.task_count,
            format!(
                "tasks={}, success={}",
                structured_summary.task_count, structured_summary.success_count
            ),
        ),
        check(
            "structured_state_transfers",
            structured_summary.state_transfer_count > 0 && structured_summary.state_bytes > 0,
            format!(
                "count={}, bytes={}",
                structured_summary.state_transfer_count, structured_summary.state_bytes
            ),
        ),
        check(
            "text_has_no_state_transfer",
            text_summary.state_transfer_count == 0,
            format!("count={}", text_summary.state_transfer_count),
        ),
        check(
            "memory_reuse",
            structured_summary.memory_queries_with_hits > 0
                && structured_summary.memory_hit_rate() > 0.0,
            format!(
                "queries_with_hits={}, hit_rate={:.2}",
                structured_summary.memory_queries_with_hits,
                structured_summary.memory_hit_rate()
            ),
        ),
        check(
            "communication_saving",
            text_saving >= min_text_saving && byte_saving >= min_byte_saving,
            format!(
                "text_saving={:.2}%, byte_saving={:.2}%, min_text={:.2}%, min_byte={:.2}%",
                text_saving * 100.0,
                byte_saving * 100.0,
                min_text_saving * 100.0,
                min_byte_saving * 100.0
            ),
        ),
        check(
            "state_files",
            state_file_count >= min_state_files,
            format!("files={state_file_count}, min={min_state_files}"),
        ),
        check(
            "memory_search_file",
            memory_hit_count >= min_memory_hits,
            format!("hits={memory_hit_count}, min={min_memory_hits}"),
        ),
        check(
            "report_file",
            report_md.exists(),
            report_md.display().to_string(),
        ),
        check(
            "observe_file",
            observe.exists(),
            observe.display().to_string(),
        ),
    ];
    let report = AuditReport::new(checks);
    report.ensure_passed()?;
    Ok(report)
}

async fn run_demo(output_dir: PathBuf, rounds: usize) -> Result<()> {
    tokio::fs::create_dir_all(&output_dir).await?;
    let memory_db = output_dir.join("memlink.sqlite");
    let state_dir = output_dir.join("state");
    let suite = PathBuf::from("suites/linked_tasks.toml");
    let text_events = output_dir.join("text-events.jsonl");
    let structured_events = output_dir.join("structured-events.jsonl");
    let text_memory_db = mode_memory_path(&memory_db, "text");
    let structured_memory_db = mode_memory_path(&memory_db, "structured");
    let report = output_dir.join("report.md");
    let observe = output_dir.join("observe.json");
    let memory_hits = output_dir.join("memory-search.json");
    let _ = tokio::fs::remove_dir_all(&state_dir).await;
    let _ = tokio::fs::remove_file(&text_events).await;
    let _ = tokio::fs::remove_file(&structured_events).await;
    let _ = tokio::fs::remove_file(&text_memory_db).await;
    let _ = tokio::fs::remove_file(&structured_memory_db).await;
    let suite = read_suite(&suite).await?;
    let experiment_id = Uuid::new_v4();
    let mut failures = Vec::new();
    for index in 0..rounds {
        let task = suite.tasks[index % suite.tasks.len()].clone();
        if let Err(error) = run_one(
            RunMode::Text,
            task.clone(),
            text_memory_db.clone(),
            text_events.clone(),
            experiment_id,
            StateBackend::MmapFile,
            state_dir.join("text"),
        )
        .await
        {
            failures.push(format!("text round {} failed: {error:#}", index + 1));
        }
        if let Err(error) = run_one(
            RunMode::Structured,
            task,
            structured_memory_db.clone(),
            structured_events.clone(),
            experiment_id,
            StateBackend::MmapFile,
            state_dir.join("structured"),
        )
        .await
        {
            failures.push(format!("structured round {} failed: {error:#}", index + 1));
        }
    }
    if !failures.is_empty() {
        anyhow::bail!("demo failed: {}", failures.join("; "));
    }
    let text_summary = summarize_events(&text_events, Some(RunMode::Text)).await?;
    let structured_summary =
        summarize_events(&structured_events, Some(RunMode::Structured)).await?;
    tokio::fs::write(
        &report,
        comparison_report(&text_summary, &structured_summary),
    )
    .await?;
    let snapshot = capture_process_snapshot("memlink demo").await?;
    write_snapshot(&observe, &snapshot).await?;
    let store = SqliteMemoryStore::open(&structured_memory_db)?;
    let query = "StateRef shared memory".to_owned();
    let hits = store
        .search(MemoryQuery {
            query: query.clone(),
            tags: vec!["stateref".to_owned(), "summary".to_owned()],
            embedding: deterministic_embedding(&query, 32),
            limit: 5,
        })
        .await?;
    tokio::fs::write(&memory_hits, serde_json::to_string_pretty(&hits)?).await?;
    let state_file_count = count_files(state_dir.clone()).await?;
    println!("demo_dir={}", output_dir.display());
    println!("report={}", report.display());
    println!("observe={}", observe.display());
    println!("memory_search={}", memory_hits.display());
    println!("state_files={state_file_count}");
    Ok(())
}

async fn run_one(
    mode: RunMode,
    task: TaskSpec,
    memory_db: PathBuf,
    events: PathBuf,
    experiment_id: ExperimentId,
    state_backend: StateBackend,
    state_dir: PathBuf,
) -> Result<memlink_runtime::TaskOutcome> {
    let state = open_state_store(state_backend, state_dir).await?;
    let memory = Arc::new(SqliteMemoryStore::open(memory_db)?);
    let evaluator = JsonlEvaluator::open(events).await?;
    let runtime = Runtime::new(state, memory, evaluator);
    runtime.run_task(experiment_id, mode, task).await
}

async fn open_state_store(
    backend: StateBackend,
    state_dir: PathBuf,
) -> Result<Arc<dyn StateStore>> {
    match backend {
        StateBackend::InMemory => Ok(InMemoryStateStore::shared()),
        StateBackend::MmapFile => Ok(MmapFileStateStore::open(state_dir).await?),
    }
}

async fn read_task(path: &PathBuf) -> Result<TaskSpec> {
    let content = tokio::fs::read_to_string(path)
        .await
        .with_context(|| format!("read task file {}", path.display()))?;
    toml::from_str(&content)
        .or_else(|_| serde_json::from_str(&content))
        .with_context(|| format!("parse task file {}", path.display()))
}

async fn read_suite(path: &PathBuf) -> Result<BenchSuite> {
    let content = tokio::fs::read_to_string(path)
        .await
        .with_context(|| format!("read suite file {}", path.display()))?;
    let suite: BenchSuite = toml::from_str(&content)
        .or_else(|_| serde_json::from_str(&content))
        .with_context(|| format!("parse suite file {}", path.display()))?;
    if suite.tasks.is_empty() {
        anyhow::bail!(
            "suite file {} must contain at least one task",
            path.display()
        );
    }
    Ok(suite)
}

fn mode_memory_path(path: &std::path::Path, mode: &str) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("memlink");
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("sqlite");
    parent.join(format!("{stem}-{mode}.{extension}"))
}

async fn count_files(root: PathBuf) -> Result<usize> {
    let mut stack = vec![root];
    let mut count = 0;
    while let Some(path) = stack.pop() {
        if !path.exists() {
            continue;
        }
        let mut entries = tokio::fs::read_dir(path).await?;
        while let Some(entry) = entries.next_entry().await? {
            let file_type = entry.file_type().await?;
            if file_type.is_dir() {
                stack.push(entry.path());
            } else if file_type.is_file() {
                count += 1;
            }
        }
    }
    Ok(count)
}

async fn read_memory_hit_count(path: &std::path::Path) -> Result<usize> {
    let content = tokio::fs::read_to_string(path)
        .await
        .with_context(|| format!("read memory search file {}", path.display()))?;
    let hits: Vec<serde_json::Value> = serde_json::from_str(&content)
        .with_context(|| format!("parse memory search file {}", path.display()))?;
    Ok(hits.len())
}
