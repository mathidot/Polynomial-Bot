# Polynomial Bot

Polynomial Bot 是一个基于 Rust 开发的高性能 Polymarket 交易机器人客户端。它专为自动做市和量化交易设计，提供了实时市场数据流、订单簿管理以及基于策略的自动交易执行功能。

## 🚀 功能特性

- **高性能架构**：基于 `Tokio` 异步运行时，利用 Rust 的零成本抽象实现低延迟处理。
- **实时数据流**：
    - 支持 Polymarket 官方 WebSocket API。
    - 实现了健壮的断线重连（Auto-reconnection）和自动重新订阅机制。
    - 区分处理 Crypto、Sports 和 User 频道数据。
- **订单簿管理**：高效的本地订单簿维护，实时同步市场深度。
- **自动交易策略**：
    - 内置 "Tail Eater" 策略，支持根据价格阈值自动下单。
    - 支持滑点保护（Slippage Protection）和最大交易量限制。
- **灵活配置**：支持实盘（REAL）和模拟（MOCK）模式切换。
- **完善的日志系统**：基于 `tracing` 的结构化日志，支持文件轮转和异步写入。
- **以太坊集成**：支持 EIP-712 签名，用于安全的订单验证。

## 📂 项目结构

```
polynomial/
├── src/
│   ├── main.rs            # 程序入口，初始化各个引擎
│   ├── lib.rs             # 库定义和模块导出
│   ├── stream.rs          # WebSocket 流处理（含重连机制）
│   ├── data_engine.rs     # 数据引擎，负责数据分发和状态更新
│   ├── execute_egine.rs   # 执行引擎，处理策略逻辑和下单
│   ├── config.rs          # 配置加载逻辑
│   └── ...                # 其他辅助模块（auth, book, types 等）
├── engine_config.toml     # 引擎运行模式配置
├── logging_config.toml    # 日志系统配置
├── strategy_config.toml   # 交易策略参数配置
├── Cargo.toml             # 项目依赖定义
└── README.md              # 项目文档
```

## 🛠️ 安装与运行

### 前置要求

- Rust (latest stable)
- Cargo

### 1. 克隆项目

```bash
git clone https://github.com/your-repo/polynomial.git
cd polynomial
```

### 2. 环境变量设置

项目依赖 `.env` 文件来加载敏感信息。请在根目录创建 `.env` 文件并设置私钥：

```env
PK=your_private_key_here
```

### 3. 配置文件

在运行前，请检查根目录下的配置文件：

**engine_config.toml** (运行模式)
```toml
# 可选值: "REAL" (实盘), "MOCK" (模拟数据)
engine_mode = "REAL"
```

**strategy_config.toml** (策略配置)
```toml
[tail_eater]
buy_threshold = "0.95"   # 触发买入的价格下限
buy_upper = "0.98"       # 触发买入的价格上限
max_slippage = "0.005"   # 允许的最大滑点 (0.5%)
trade_unit = "100"       # 每次交易的数量
```

**logging_config.toml** (日志配置)
```toml
log_dir = "logs"            # 日志输出目录
log_file = "polynomial.log" # 日志文件名
level = "info"              # 日志级别 (trace, debug, info, warn, error)
```

### 4. 运行项目

```bash
cargo run --release
```

## 🏗️ 架构概览与核心路径

系统主要由以下核心组件构成，并遵循严格的数据流处理路径：

### 核心组件

1.  **WebSocketStream (Stream Actor)**:
    *   负责维护与 Polymarket 的 WebSocket 连接。
    *   内置连接管理器（Connection Manager），处理心跳、断线重连和订阅恢复。
    *   将原始消息解析为 `StreamMessage` 并传递给数据层。

2.  **DataEngine (数据引擎)**:
    *   订阅并管理多个频道的流（Crypto, Sports, User）。
    *   接收 WebSocket 消息，更新全局状态（`GlobalState`）。
    *   维护本地订单簿（OrderBook）和最新价格。

3.  **ExecuteEngine (执行引擎)**:
    *   监听来自 `DataEngine` 的价格信号（`TokenInfo`）。
    *   运行策略逻辑（如 Tail Eater）。
    *   在本地订单簿上进行预执行模拟（Simulation），检查滑点。
    *   通过 `ClobClient` 向交易所发送实际订单。

### 核心交易路径 (Message to Order)

从接收市场消息到完成下单的完整路径如下：

1.  **消息接收 (Stream Layer)**:
    *   `WebSocketStream` 接收 JSON 数据 -> 反序列化为 `StreamMessage` -> `StreamProvider` 分发。

2.  **状态更新 (Data Engine)**:
    *   `DataEngine` 获取 `GlobalState` 锁 -> 更新内存中的 `OrderBook` -> 提取最新价格 -> 发送 `TokenInfo` 信号。

3.  **策略决策 (Execute Engine)**:
    *   `ExecuteEngine` 收到信号 -> 检查阈值 (`buy_threshold`) -> 再次获取 `GlobalState` 锁读取 `OrderBook` -> 调用 `FillEngine` 进行模拟撮合 -> 计算预期滑点 -> 通过/拒绝。

4.  **下单执行 (Client Layer)**:
    *   构建订单参数 -> `ClobClient` 进行 EIP-712 签名 -> 发送 HTTP POST 请求 -> 返回 Order ID。

## ⚡ 性能基准测试 (Benchmarks)

本项目包含性能基准测试，用于验证核心路径的延迟。

### 运行测试

```bash
cargo bench --bench full_path_benchmark
```

### 测试结果示例

针对“数据更新 -> 策略决策 -> 模拟撮合”的全路径测试（不含网络 IO）：

```
full_execution_path     time:   [14.789 µs 15.028 µs 15.285 µs]
```

*   **平均延迟**: ~15 微秒 (0.015ms)
*   **测试范围**: 包含状态锁竞争、OrderBook 更新、定点数计算、策略逻辑判断及本地模拟撮合。

## ⚠️ 注意事项

*   **私钥安全**：请务必保护好您的 `.env` 文件，不要将其提交到版本控制系统中。
*   **风险提示**：自动化交易涉及资金风险，建议先在 `MOCK` 模式下测试策略逻辑，或使用小额资金进行测试。

## 📝 License

[MIT License](LICENSE)
