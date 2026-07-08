# MemLink MVP 实验报告

## 环境

- Rust workspace edition：2024
- 命令：`cargo run -p memlink-cli -- demo --rounds 10 --output-dir reports/demo`
- 任务套件：`suites/linked_tasks.toml`
- 轮数：10
- 对照：text 与 structured 使用相同任务 suite，bench 内部使用独立 SQLite 数据库隔离记忆污染；当前 MVP 仅 structured mode 写入和复用共享记忆，text mode 作为无共享记忆基线。

## 最新验证结果

| Metric | Text | Structured | Improvement |
| --- | ---: | ---: | ---: |
| Tasks | 10 | 10 | - |
| Success | 10 | 10 | - |
| Messages | 68 | 148 | - |
| Text chars | 50820 | 19294 | 62.03% |
| Encoded bytes | 55776 | 46836 | 16.03% |
| State transfers | 0 | 80 | - |
| State bytes | 0 | 149806 | - |
| Duration ms | 180 | 228 | -26.67% |
| Memory queries with hits | 0 | 9 | - |
| Memory hit rate | 0.00% | 90.00% | - |
| Effective reuse rate | 0.00% | 94.29% | - |

## 结论

- Structured 模式使用 `ActionRequest`、`ActionResult`、`MemoryHit` 和 `StateRef` 替代大段自然语言上下文，在当前 suite 下减少约 62% 文本字符与约 16% 编码字节。
- Structured 模式通过 `StateRef` 传递 evidence pack、embedding 和 tool output，10 轮共产生 80 次非文本状态传递。
- SQLite 共享记忆在 structured 连续任务中形成跨任务命中，达到 90% 查询命中率；text mode 不写入或复用共享记忆，作为无共享记忆基线。
- 当前实现已包含 Unix Socket transport、受限 CodeAct 子进程执行后端与轻量观测快照；mmap/FD passing、WASM/WASI 与 eBPF 仍保留为后续增强。
