# MemLink 验收矩阵

本文按 `docs/ARCHITECTURE.md` 11.2/11.3 对当前 Rust 2024 实现做验收映射。

## 基础验收

| 要求 | 当前实现 | 证据 |
| --- | --- | --- |
| 至少 3 个 Agent 协作完成多步骤流程 | 默认启用 Planner、Retriever、Executor、Summarizer 四类 Agent | `crates/memlink-runtime/src/lib.rs`、`crates/memlink-runtime/tests/runtime_flow.rs` |
| 支持 text 与 structured 双模式并生成对比报告 | CLI `bench` 对同一 suite 运行两种模式并输出 report | `crates/memlink-cli/src/main.rs`、`docs/EXPERIMENT_REPORT.md` |
| 结构化消息包含动作、输入、结果、能力描述 | `ActionRequest`、`ActionResult`、`Capability`、`Message` | `crates/memlink-protocol/src/lib.rs` |
| 支持握手、能力发现、协议路由、错误返回 | Runtime 注册 capability，按角色调度；协议含 `ErrorPayload` | `crates/memlink-runtime/src/lib.rs`、`crates/memlink-protocol/src/lib.rs` |
| 支持非文本状态传递并统计次数/规模 | `StateRef` 传递 evidence、embedding、tool output；event log 统计 `StateTransfer` | `crates/memlink-state/src/lib.rs`、`crates/memlink-evaluator/src/lib.rs` |
| 共享记忆包含 ID、来源、时间、主题、摘要 | `MemoryUnit` 持久化 SQLite | `crates/memlink-memory/src/lib.rs` |
| 支持关键词、标签、语义相似度检索和复用 | SQLite tags/keywords + deterministic embedding cosine search + reuse event | `crates/memlink-memory/src/lib.rs`、`crates/memlink-memory/tests/sqlite_memory.rs` |
| 至少 2 组关联连续任务 | A 组知识检索总结，B 组代码分析策略复用 | `suites/linked_tasks.toml` |
| 稳定执行不少于 10 轮连续任务 | `bench --rounds 10` 和 `demo --rounds 10` | `docs/EXPERIMENT_REPORT.md` |
| 输出消息数、文本开销、状态传递、耗时、命中率、提升 | evaluator 生成 JSONL 与 Markdown report | `crates/memlink-evaluator/src/lib.rs`、`reports/report.md`（运行生成） |

## 加分验收

| 要求 | 当前实现 | 证据 |
| --- | --- | --- |
| Agent 间通信支持 Unix Domain Socket | `UnixSocketTransport` 发送结构化 `TransportFrame` | `crates/memlink-transport/src/lib.rs`、`crates/memlink-transport/tests/unix_socket.rs` |
| 大状态支持 mmap/FD passing/POSIX shared memory | `MmapFileStateStore` file-backed 大状态，`StateRef.transport = MmapFile` | `crates/memlink-state/src/lib.rs`、`crates/memlink-state/tests/state_store.rs` |
| CodeAct 通过 WASM/WASI 或更强沙箱执行 | 当前采用受限子进程后端：临时目录、清理环境、超时、输出限制，Unix 下附加进程组清理与资源限制；不是 WASM/容器级强隔离 | `crates/memlink-sandbox/src/lib.rs`、`crates/memlink-sandbox/tests/restricted_process.rs` |
| 使用 eBPF 或系统观测工具展示进程/socket/IO/沙箱行为 | `observe` 命令输出进程、socket/fd 快照 | `crates/memlink-observe/src/lib.rs`、`crates/memlink-observe/tests/process_snapshot.rs` |
| 支持迁移到多进程 Agent 部署 | Protocol、StateStore、Transport、Sandbox 均为 trait/独立 crate，Unix socket 已可传 frame | `crates/memlink-protocol`、`crates/memlink-transport` |

## 一键演示

```bash
cargo run -p memlink-cli -- demo --rounds 10 --output-dir reports/demo
cargo run -p memlink-cli -- audit --input-dir reports/demo --min-tasks 10 --min-state-files 1 --min-memory-hits 1
```

演示会生成：

- `reports/demo/report.md`：text vs structured 对比报告。
- `reports/demo/text-events.jsonl` 与 `reports/demo/structured-events.jsonl`：可复算事件日志。
- `reports/demo/observe.json`：系统观测快照。
- `reports/demo/memory-search.json`：共享记忆检索示例。
- `reports/demo/state/`：mmap/file-backed 状态文件。

## 提交级验证命令

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo run -p memlink-cli -- demo --rounds 10 --output-dir reports/demo
cargo run -p memlink-cli -- audit --input-dir reports/demo --min-tasks 10 --min-state-files 1 --min-memory-hits 1
```

## 机器可读审计

`audit` 命令会读取 demo 产物并输出 JSON，检查：

- text/structured 任务数和成功数。
- structured 状态传递次数与字节数。
- text 模式没有状态传递。
- structured 记忆查询命中。
- structured 通信字符/编码字节相对 text 有节省。
- mmap/file-backed 状态文件存在。
- 记忆检索示例存在命中结果。
- report 与 observe 文件存在。

## 完成审计

参见 `docs/COMPLETION_AUDIT.md`。
