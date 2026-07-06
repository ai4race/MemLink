# MemLink Deployment

## Prerequisites

- Rust stable with 2024 edition support.
- Linux/macOS shell for the MVP local runtime.

## Build

```bash
cargo build --workspace
```

## Single Task

```bash
cargo run -p memlink-cli -- run --mode structured --task-file tasks/a1.toml
cargo run -p memlink-cli -- run --mode text --task-file tasks/a1.toml
```

## 10-Round Benchmark

```bash
cargo run -p memlink-cli -- bench --suite suites/linked_tasks.toml --rounds 10 --output-dir reports
```

Outputs:

- `reports/text-events.jsonl`
- `reports/structured-events.jsonl`
- `reports/report.md`
- `data/memlink.sqlite`

## Memory Search

```bash
cargo run -p memlink-cli -- memory search --query "StateRef shared memory" --tags stateref,summary
```

## Mmap/File State Backend

```bash
cargo run -p memlink-cli -- bench --suite suites/linked_tasks.toml --rounds 10 --state-backend mmap-file --state-dir data/state --output-dir reports
```

`mmap-file` 后端将大状态写入 `data/state/{text,structured}`，消息中只携带 `StateRef` 元数据。
