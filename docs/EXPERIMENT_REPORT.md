# MemLink MVP 实验报告

## 环境

- Rust workspace edition：2024
- 命令：`cargo run -p memlink-cli -- demo --rounds 10 --output-dir reports/demo`
- 任务套件：`suites/linked_tasks.toml`
- 轮数：10
- 对照：text 与 structured 使用相同任务 suite，bench 内部使用独立 SQLite 数据库隔离记忆污染。

## 最新验证结果

| Metric | Text | Structured | Improvement |
| --- | ---: | ---: | ---: |
| Tasks | 10 | 10 | - |
| Success | 10 | 10 | - |
| Messages | 68 | 108 | - |
| Text chars | 61201 | 41263 | 32.58% |
| Encoded bytes | 63599 | 40391 | 34.00% |
| State transfers | 0 | 104 | - |
| State bytes | 0 | 170119 | - |
| Duration ms | 210 | 207 | 1.43% |
| Memory queries with hits | 9 | 9 | - |
| Memory hit rate | 90.00% | 90.00% | - |
| Effective reuse rate | 100.00% | 67.65% | - |

## 结论

- Structured 模式使用 `ActionRequest`、`ActionResult`、`MemoryHit` 和 `StateRef` 替代大段自然语言上下文，在当前 suite 下减少约 33% 文本字符与约 34% 编码字节。
- Structured 模式通过 `StateRef` 传递 evidence pack、embedding 和 tool output，10 轮共产生 104 次非文本状态传递。
- SQLite 共享记忆在连续任务中形成跨任务命中，text 与 structured 两种模式均达到 90% 查询命中率。
- 当前实现已包含 Unix Socket transport、受限 CodeAct 子进程沙箱与轻量观测快照；mmap/FD passing、WASM/WASI 与 eBPF 仍保留为后续增强。
