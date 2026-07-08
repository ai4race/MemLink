# MemLink 完成审计

本审计基于当前 Rust 2024 workspace、`docs/ARCHITECTURE.md`、`docs/ACCEPTANCE.md` 和可执行验证命令。

## 验证命令

正式验证：

```bash
scripts/verify.sh
```

该命令默认执行 10 轮 demo，并调用：

```bash
cargo run -p memlink-cli -- audit \
  --input-dir reports/demo \
  --min-tasks 10 \
  --min-state-files 1 \
  --min-memory-hits 1 \
  --min-text-saving 0.0 \
  --min-byte-saving 0.0
```

快速 smoke 可降低轮数，但短轮数指标波动较大，若只做冒烟测试可设置：

```bash
MEMLINK_DEMO_ROUNDS=2 MEMLINK_MIN_TEXT_SAVING=-1.0 scripts/verify.sh
```

## 证明项

| 要求 | 证明方式 | 当前状态 |
| --- | --- | --- |
| Rust 2024 workspace | `Cargo.toml` workspace package edition = 2024 | 已满足 |
| 多 Agent 协作 | runtime 默认 Planner/Retriever/Executor/Summarizer；`runtime_flow` 测试 | 已满足 |
| 结构化协议 | `memlink-protocol` 定义 `Message`、`ActionRequest`、`ActionResult`、`Capability`、`StateRef` | 已满足 |
| text/structured 双模式 | `memlink demo` 同 suite 跑两种模式并生成 report | 已满足 |
| 非文本状态传递 | structured 产生 `StateTransfer` event，audit 检查 count/bytes | 已满足 |
| mmap/file-backed 大状态 | `MmapFileStateStore`，demo 生成 `reports/demo/state` 文件 | 已满足 |
| 共享记忆存储检索复用 | SQLite memory store + keyword/tag/semantic search + reuse event | 已满足 |
| 两组关联连续任务 | `suites/linked_tasks.toml` A/B 两组任务 | 已满足 |
| 10 轮连续任务 | `scripts/verify.sh` 默认 `demo --rounds 10`，audit 检查 min tasks | 已满足 |
| 性能统计展示 | `reports/demo/report.md` 与 JSONL event log | 已满足 |
| Unix Domain Socket | `memlink-transport` + `unix_socket` 测试 | 已满足 |
| CodeAct 受限执行 | `memlink-sandbox` 受限子进程，Unix 下附加进程组清理与 rlimit，Executor 接入 | 已满足 MVP；不声称 WASM/容器级强隔离 |
| 系统观测 | `memlink observe` 与 demo `observe.json` | 已满足 |
| 部署/提交文档 | `docs/DEPLOYMENT.md`、`docs/SUBMISSION.md` | 已满足 |
| 演示视频材料 | `docs/DEMO_SCRIPT.md` 与 `scripts/record_demo.sh` 提供录制脚本和 transcript 生成 | 已满足素材，二进制视频可按脚本录制 |

## 未声称完成的增强

以下属于后续可替换/增强项，当前已有接口或退化实现，但不声称已完整实现：

- 真实 POSIX shared memory 与 FD passing。
- WASM/WASI backend；当前是受限子进程执行后端，不应用于执行不可信代码。
- eBPF/aya 程序；当前是轻量系统观测快照。

## 结论

当前仓库已满足 `docs/ARCHITECTURE.md` 中 MVP、基础验收和大部分 M4 加分原型要求，并提供可重复运行的机器审计命令。若赛题提交必须包含真实视频文件，则需按 `docs/DEMO_SCRIPT.md` 录制；源码侧已提供完整演示流程和 `scripts/record_demo.sh` 终端 transcript 生成。
