# 赛项介绍
一种面向多智能体协作的低开销通信、状态传递与共享记忆机制（社区赛题）

# 赛题说明

随着大模型应用从单 Agent 问答逐步扩展到多 Agent 协同执行，智能系统正在从“单点生成”向“分工协作”演进。在检索增强生成、复杂任务规划、代码协作、办公自动化、知识分析等场景中，往往需要多个 Agent 分别承担规划、检索、执行、总结、生成等不同角色，并通过相互协作完成复杂任务。当前主流多 Agent 系统大多以自然语言或 JSON 作为通信媒介，即一个 Agent 将其中间结果组织成文本，再传递给其他 Agent 进行解析和继续处理。这种方式虽然通用性较好，但在多轮、多 Agent、复杂任务场景下存在明显不足：一是通信内容冗长、重复上下文多，token 消耗高；二是中间结果需要在“内部状态—文本—内部状态”之间反复转换，导致时延增加并可能带来语义损耗；三是任务执行过程中形成的中间知识和经验难以沉淀，系统在处理相似任务时往往仍需从头开始，缺乏持续积累和复用能力。


本赛题面向多智能体协作系统中的基础设施问题，要求选手围绕**低开销通信、非文本状态传递、共享记忆复用**三个方面，设计并实现一套可运行的原型系统。系统一方面需要通过结构化通信协议替代冗长自然语言交互，将 Agent 间传递的内容收敛为动作、参数、结果、能力等高密度语义单元，以降低通信成本和解析开销；另一方面需要探索 embedding、语义向量、隐藏状态特征或其他中间表示在 Agent 之间的直接传递机制，减少不必要的文本编解码过程，提高协作效率。在此基础上，还需将任务执行过程中形成的摘要、证据、策略、经验等内容沉淀为可标识、可检索、可复用的共享记忆单元，使系统具备跨任务的知识积累和协同增强能力。


本课题区别于一般的工作流编排类题目，重点不在于简单调用大模型接口和外部工具，而在于研究多智能体协作中的“系统层机制”：包括 Agent 间统一通信协议设计、中间状态表示与交换方式、共享记忆组织模型、跨任务复用机制以及整体运行效率验证。选手需面向开源操作系统或通用 Linux 环境完成原型实现，通过可复现实验验证该机制相较传统纯文本协作方式在通信开销、任务时延和记忆复用方面的改进效果。        


# 具体要求

系统需支持不少于 3 个 Agent 协同运行，至少覆盖任务规划、信息检索、总结生成、工具执行等角色中的 3 类，并能够完成一个包含多步骤处理过程的复杂任务；
系统需设计并实现一套面向 Agent 间协作的结构化通信机制，通信内容至少包括动作类型、输入参数、返回结果和能力描述，并支持基本的握手、能力发现或协议映射机制，不得仅通过自然语言长文本直接透传全部协作信息；
系统需同时支持“纯文本协作模式”和“结构化协议协作模式”，并在相同任务条件下完成可复现实验对比；
系统需实现一种非文本中间状态传递机制，支持 embedding、语义向量、隐藏状态特征或其他中间表示在 Agent 间直接交换，并说明其生成方式、传递方式、接收方式及后续使用方式；
系统需实现共享记忆模块，能够将任务执行过程中的中间结果、摘要、经验片段、证据链、结论或策略保存为统一的记忆单元，并为每条记忆记录至少包含记忆 ID、来源 Agent、创建时间、任务主题和摘要描述等基本元数据；
系统需支持按关键词、标签或语义相似度检索历史记忆，并允许不同 Agent 在后续任务中直接复用已有记忆；
系统需至少设计 2 组具有关联性的连续任务，验证结构化通信、非文本状态传递和共享记忆复用在减少重复计算、降低协作开销和提升任务效率方面的实际效果；
系统需统计并展示 Agent 间消息次数、文本通信 token 或字符开销、非文本状态传递次数及数据规模、单任务总耗时、共享记忆命中率及整体性能提升情况；
系统架构中至少应包含多 Agent 运行时、协议解析与调度模块、状态交换模块、共享记忆存储与检索模块和评测模块，并能够稳定执行不少于 10 轮连续任务；
需提交完整源码、系统设计文档、部署文档、实验报告和演示视频，能够支持评审现，鼓励结合 IPC、共享内存、Socket、向量数据库、WASM/容器沙箱、eBPF 等系统技术提升实现质量。
鼓励系统能够支持基于 CodeAct 模式的 Agent 执行机制，允许 LLM 生成 Python 可执行代码，并通过轻量执行隔离或沙箱实现低延迟、可审计的结果回传能力。


# 赛题要求

系统支持不少于3个Agent协同运行，覆盖规划、检索、执行、总结等角色；
设计结构化通信协议替代自然语言交互；
实现非文本中间状态传递机制（embedding/语义向量/隐藏状态）；
实现共享记忆模块，支持记忆的存储、检索和复用；
至少设计2组关联性连续任务进行验证；
提供通信开销、任务时延、记忆复用等方面的性能对比数据。


## 评分细则（明确评审角度、标准和分值范围）：

通信效率（25分）：相比纯文本协作的token节省效果
状态传递创新（20分）：非文本状态传递机制的设计新颖性
记忆复用效果（20分）：跨任务记忆复用的准确性与效率
系统完整性（20分）：多Agent协作的稳定性与功能覆盖
实验验证（15分）：性能对比数据的说服力


# MemLink Rust 2024 MVP

本仓库已按 `docs/ARCHITECTURE.md` 落地 Rust 2024 Edition MVP workspace，覆盖协议、运行时、状态交换、共享记忆、评测与 CLI。

## Workspace

- `crates/memlink-protocol`：`RunMode`、`Message`、`Capability`、`ActionRequest`、`ActionResult`、`StateRef` 等 wire model。
- `crates/memlink-runtime`：Planner、Retriever、Executor、Summarizer 四类 Agent 与单机异步 dispatcher。
- `crates/memlink-state`：进程内 `StateStore`、BLAKE3 checksum、deterministic embedding 与 `StateRef` 传递。
- `crates/memlink-memory`：SQLite 记忆单元、标签/关键词索引、确定性向量语义检索、reuse event。
- `crates/memlink-evaluator`：JSONL 事件日志、指标汇总、text/structured 对比报告。
- `crates/memlink-cli`：`run`、`bench`、`memory search`、`report` 命令。

## Quick Start

```bash
cargo test --workspace
cargo run -p memlink-cli -- run --mode structured --task-file tasks/a1.toml
cargo run -p memlink-cli -- bench --suite suites/linked_tasks.toml --rounds 10 --output-dir reports
```

## Outputs

- `reports/text-events.jsonl`：纯文本模式事件日志。
- `reports/structured-events.jsonl`：结构化协议模式事件日志。
- `reports/report.md`：消息数、字符开销、编码字节、状态传递、耗时、记忆命中率对比。
- `data/memlink.sqlite`：共享记忆、标签/关键词索引和复用事件。

## Memory Search

```bash
cargo run -p memlink-cli -- memory search --query "StateRef shared memory" --tags stateref,summary --limit 3
```

## Benchmark Suite

`suites/linked_tasks.toml` 包含两组关联连续任务：

- A 组：知识检索、追问复用、对比总结。
- B 组：代码片段分析、相似 bug 策略复用、策略型记忆沉淀。

`bench --rounds 10` 会在同一 suite 上分别运行 text 与 structured 模式，失败不中断，并输出可复算事件与报告。

## Enhanced Modules

- `crates/memlink-sandbox`：受限 CodeAct 子进程后端，Executor 可用它执行 Python/Shell 分析片段；不是 WASM/容器级强隔离。
- `crates/memlink-transport`：Unix Domain Socket 结构化 frame 传输，保留 `StateRef` 列表用于多进程扩展。
- `crates/memlink-observe`：系统观测快照，支持 CLI 输出 `reports/observe.json`。

```bash
cargo run -p memlink-cli -- observe --output reports/observe.json --note "demo"
```

## Mmap/File State Backend

```bash
cargo run -p memlink-cli -- bench --suite suites/linked_tasks.toml --rounds 10 --state-backend mmap-file --state-dir data/state --output-dir reports
```

该模式会把 evidence、embedding、tool output 写入文件状态仓库，并在消息中传递 `StateRef`。

## Acceptance And Demo

- 验收矩阵：`docs/ACCEPTANCE.md`
- 演示脚本：`docs/DEMO_SCRIPT.md`
- 一键演示：

```bash
cargo run -p memlink-cli -- demo --rounds 10 --output-dir reports/demo
```

## Submission

- 提交说明：`docs/SUBMISSION.md`
- 本地验证：

```bash
scripts/verify.sh
```

- 生成源码包：

```bash
scripts/package.sh
```

## Machine Audit

```bash
cargo run -p memlink-cli -- audit --input-dir reports/demo --min-tasks 10 --min-state-files 1 --min-memory-hits 1
```

`audit` 输出 JSON 审计结果，可用于评审自动判断 demo 产物是否满足核心验收指标。

- 完成审计：`docs/COMPLETION_AUDIT.md`

## Demo Recording Material

```bash
scripts/record_demo.sh reports/demo-recording
```

生成终端演示 transcript，便于录制或提交辅助演示材料。
