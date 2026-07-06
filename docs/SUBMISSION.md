# MemLink 提交说明

## 提交内容

- 完整 Rust 2024 workspace 源码：`Cargo.toml`、`Cargo.lock`、`crates/`
- 系统设计文档：`docs/ARCHITECTURE.md`
- 部署文档：`docs/DEPLOYMENT.md`
- 增强项说明：`docs/ENHANCEMENTS.md`
- 验收矩阵：`docs/ACCEPTANCE.md`
- 完成审计：`docs/COMPLETION_AUDIT.md`
- 实验报告：`docs/EXPERIMENT_REPORT.md`
- 演示视频脚本：`docs/DEMO_SCRIPT.md`
- 任务套件：`suites/linked_tasks.toml`
- 单任务示例：`tasks/a1.toml`
- 验证与打包脚本：`scripts/verify.sh`、`scripts/package.sh`

## 评审复现

```bash
scripts/verify.sh
```

该命令会执行：

1. `cargo fmt --all -- --check`
2. `cargo test --workspace`
3. `cargo run -p memlink-cli -- demo --rounds 10 --output-dir reports/demo`
4. `cargo run -p memlink-cli -- audit --input-dir reports/demo --min-tasks 10 --min-state-files 1 --min-memory-hits 1`
5. 校验 report、event log、observe、memory search 和 state files 是否生成。

## 生成提交包

```bash
scripts/package.sh
```

输出：

- `dist/memlink-rust2024.tar.gz`
- `dist/memlink-rust2024.tar.gz.sha256`

压缩包不包含 `target/`、`data/`、`reports/`、`dist/` 和 `.git/`。

## 演示录制

```bash
scripts/record_demo.sh reports/demo-recording
```

该命令生成 `reports/demo-recording/terminal-demo.txt` 和对应命令脚本，可作为录屏素材或终端演示证据。

## 演示视频建议

按照 `docs/DEMO_SCRIPT.md` 录制即可。建议视频结构：

1. 展示架构目标和 workspace。
2. 运行 `scripts/verify.sh` 或 `memlink demo`。
3. 展示 `reports/demo/report.md` 对比指标。
4. 展示 `reports/demo/memory-search.json` 记忆复用。
5. 展示 `reports/demo/state/` 状态文件。
6. 说明 Unix Socket、Sandbox、mmap/file state、Observe 增强项测试。

## 当前增强范围

已实现：

- Unix Domain Socket frame transport。
- mmap/file-backed 状态仓库。
- 受限 CodeAct 子进程沙箱。
- 轻量系统观测快照。

后续可扩展：

- POSIX shared memory + FD passing。
- WASM/WASI sandbox backend。
- aya/libbpf eBPF demo。
