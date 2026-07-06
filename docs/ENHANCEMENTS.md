# MemLink 系统技术增强说明

本文件对应 `docs/ARCHITECTURE.md` 的 M4/加分项落地状态。

## Unix Domain Socket Transport

- Crate：`crates/memlink-transport`
- 能力：`UnixSocketTransport` 通过 Unix Domain Socket 发送 `TransportFrame`，frame 中包含结构化 `Message` 与 `StateRef` 列表。
- 验证：`crates/memlink-transport/tests/unix_socket.rs`

## CodeAct Restricted Process Sandbox

- Crate：`crates/memlink-sandbox`
- 能力：`RestrictedProcessSandbox` 在临时目录、清理后的环境变量、超时和输出大小限制下执行 Python/Shell 片段。
- Runtime 接入：`ExecutorAgent` 使用 sandbox 分析代码片段，并将结果写入 tool output state。
- 验证：`crates/memlink-sandbox/tests/restricted_process.rs` 与 `crates/memlink-runtime/tests/runtime_flow.rs`

## Observability Snapshot

- Crate：`crates/memlink-observe`
- 能力：采集当前进程 ID、Unix socket 计数和 fd 计数；Linux 下读取 `/proc/net/unix` 与 `/proc/self/fd`，非 Linux 退化为可解释快照。
- CLI：`cargo run -p memlink-cli -- observe --output reports/observe.json --note "demo"`
- 验证：`crates/memlink-observe/tests/process_snapshot.rs`

## Mmap/File State Backend

- Crate：`crates/memlink-state`
- 能力：`MmapFileStateStore` 将大状态落盘为文件，`StateRef.transport` 标记为 `MmapFile`，读取时执行 BLAKE3 checksum 校验，并支持 TTL 清理。
- CLI：`cargo run -p memlink-cli -- bench --suite suites/linked_tasks.toml --rounds 10 --state-backend mmap-file --state-dir data/state --output-dir reports`
- 验证：`crates/memlink-state/tests/state_store.rs`

## 仍保留为后续演进

- POSIX shared memory 与 FD passing：当前已实现 mmap/file-backed 状态后端，但尚未实现真实跨进程 FD 传递。
- WASM/WASI：当前先用受限子进程满足 CodeAct 隔离 MVP，后续可替换 `Sandbox` trait 实现。
- eBPF：当前提供轻量系统观测快照，后续可在 `memlink-observe` 中接入 aya/libbpf。
