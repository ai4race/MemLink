# MemLink 演示视频脚本

## 1. 开场说明

展示 `docs/ARCHITECTURE.md`，说明 MemLink 目标：结构化协议压缩通信、`StateRef` 非文本状态传递、SQLite 共享记忆复用、内建评测报告。

## 2. 构建与测试

```bash
cargo test --workspace
```

说明 Rust 2024 workspace 包含 protocol/runtime/state/memory/evaluator/cli/sandbox/transport/observe。

## 3. 一键演示

```bash
cargo run -p memlink-cli -- demo --rounds 10 --output-dir reports/demo
```

重点讲解输出：

- `state_files` 证明 file-backed `StateRef` 状态落盘。
- `report.md` 证明 text 与 structured 对照。
- `memory-search.json` 证明后续任务复用历史记忆。
- `observe.json` 证明系统观测快照。

## 4. 机器审计

```bash
cargo run -p memlink-cli -- audit --input-dir reports/demo --min-tasks 10 --min-state-files 1 --min-memory-hits 1
```

说明 `passed=true` 表示 demo 产物满足基础验收关键指标。

## 5. 查看报告

```bash
cat reports/demo/report.md
```

讲解指标：消息数、文本字符、编码字节、状态传递次数/字节、耗时、记忆命中率、提升比例。

## 6. 查看记忆检索

```bash
cat reports/demo/memory-search.json
```

说明记忆单元包含 ID、主题、摘要、分数、标签和命中原因。

## 7. 查看状态文件

```bash
find reports/demo/state -type f | head
```

说明大对象通过 `StateRef` 引用传递，实际 evidence/embedding/tool output 保存在状态仓库。

## 8. 增强项说明

- Unix Domain Socket：`crates/memlink-transport/tests/unix_socket.rs`
- 受限 CodeAct 沙箱：`crates/memlink-sandbox/tests/restricted_process.rs`
- mmap/file 状态后端：`crates/memlink-state/tests/state_store.rs`
- 系统观测：`crates/memlink-observe/tests/process_snapshot.rs`

## 自动生成终端演示素材

```bash
scripts/record_demo.sh reports/demo-recording
```

生成的 `terminal-demo.txt` 可用于提交辅助材料或作为录屏旁白依据。
