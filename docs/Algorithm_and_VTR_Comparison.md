# FDE 算法综述与 VTR 差异对比

本文面向课堂讲解，系统介绍 FDE 项目各阶段（映射/打包/布局/路由/时序/比特流）算法实现要点，并与开源 VTR (Verilog-to-Routing) 流程进行对比，突出关键差异与特色。

---

## 总览

- 流程模块：
  - 解析与通用基类：`common/`（对象模型、RRG、时序引擎 TEngine 等）
  - 技术映射：`mapping/`（AIG 转换、割枚举、LUT 生成、模式匹配）
  - 打包：`packing/`（暂未深入查看源文件，推测为 Slice/Macro 聚合）
  - 布局：`placer/`（基于模拟退火 SA，支持时序驱动）
  - 路由：`router/`（时序驱动与 BFS/定向搜索两种模式，基于自建 RRG）
  - 时序分析：`sta/`（基于 `common/tengine` 通用时序引擎实现 STA）
  - 比特流生成：`bitgen/`
  - 可视化：`viewer/`
- 顶层构建：`CMakeLists.txt` 将各子模块独立可编译。

---

## 技术映射（`mapping/`）

### 核心流程

- 入口：`mapping/main.cpp` 调用 `MappingManager`：
  - `doReadDesign()`：加载单元库和输入网表（支持 xml/verilog/edif，从 Yosys 的 EDIF 转换 INIT 属性为 LUT truth table）。
  - `doAigTransform()`：将组合模块 COMB 转换为 AIG（`aigtransform.cpp`）。
  - `doMapCut()`：对 AIG 执行割枚举与最优割选择，生成 LUT 实例并回接网络。
  - `doPtnMatch()`：简单模式处理（插入 BUF、创建常量），作为收尾清理。
  - `doWriteDesign()`/`doReport()`：输出设计与统计报表。

### AIG 转换（`aigtransform.cpp`）

- 将 COMB 模块的真值表（`MODULE` 级属性 `truthtable`）解析为若干积项（cubes），并用 AND/OR/INV 构造 AIG：
  - `AigManager::makeAigModule`：为每个 COMB 创建 `_Aig` 模块（类型 `AIG`），复制 I/O 端口；将真值表拆成输入布尔向量，逐项构造树。
  - `makeNodeTree`：将若干输入 IPin 归约为一棵 AND/OR 树。
  - 每个 AIG 节点有输入 `A/B` 与输出 `Y`，并在 `INSTANCE` 级保存 `truthtable_items`（局部真值标识）。

### 割枚举与代价（`mapping.cpp`）

- 数据结构 `MapCut`：描述一个候选割，含叶子 pins、签名、真值表、层级 level、面积 area、有效性 valid。
- 割生成：
  - 自底向上 DFS：`getNodeDfs` 在 AIG 图上按 `isDfsRoot` 根集合进行遍历，得到后序列表。
  - 对每个节点 `enumNode`：
    - 收集两侧输入的所有割集合 `getAllCuts(node,"A/B")`（包括叶割与子节点的历史割）。
    - 对 A/B 的任意组合构造新割：
      - 叶集合去重并限制在 LUT 宽度 `lutSize` 以内（同时用叶签名的位计数 `oneCount(signature)` 早停）。
      - 计算新割的真值表 `calcTruthtable`：按叶索引映射对两侧子割真值表进行合成，并用当前根的 `ttRoot`（本节点布尔功能）筛选结果。
      - 代价：`level = max(leaf.level) + 1`，`area = sum(leaf.area) + 1`（单位面积计数），并标记有效。
    - 在每个节点保留最多 63 个候选割（`addCut` 以 level/area 排序丢弃最差）。
    - 选择最优割：`min_element(betterCut)`，以二元序 `(level, area)` 比较。
- LUT 生成：
  - `isLutRoot`/`getLutRoots`：选择作为 LUT 根的 AIG 节点（其输出连接到非 AIG 或模块端口），并向下包含其叶子输入的 AIG 节点，形成转换集。
  - `createLutModule`：在 `cell_lib` 中生成 `LUT2..LUTK` 模块定义，端口命名为 `ADR0..ADRK-1` 与 `O`。
  - `makeLut`：为每个根节点实例化 LUT，连接叶子网络、设置 INIT 属性（`setTruthtableProperty` 将 2..5 输入 LUT 的真值表转成十六进制字串，必要时拆分 `INIT_1/INIT_2`），并将 AIG 输出网改名。

### 报表

- `countCells` 遍历顶层实例，统计 `LUT`、`FFLATCH`、`MACRO` 等类别与具体型号数量；`MappingManager::doReport()` 写出 `.rpt`。

### 与 VTR 对比（映射阶段）

- 输入模型：
  - FDE 使用自研 `COS::Design`/`Library`/`Module`/`Instance` 对象模型；AIG 化在项目内部完成。
  - VTR 通常依赖学界的 ABC 做技术映射与 AIG/割搜索（`vtr_flow` 调用 `abc`），割选择可包含延迟或面积权衡、更复杂的 cost 函数。
- 割枚举与选择：
  - FDE 明确实现了双输入 AIG 的全组合割枚举，代价是双目标（level 优先、area 次之）且截断为 63 个；真值表合成在本地实现。
  - VTR/ABC 提供 K-LUT 映射的先进割枚举与代价优化（含 required times/arrival，Power-aware 等），并支持 DAG-aware 重构、共享计算等优化。
- 输出：
  - FDE 在自身库中创建 `LUTK` 模块，设置 `INIT` 属性；
  - VTR 生成 .blif/.net 文件进入 pack/place/route 流后续流程。

---

## 布局（`placer/`）

### 算法要点（`plc_algorithm.cpp`）

- 模拟退火 SA，支持两种模式：Bounding-Box 与 Timing-Driven。
- 成功接受判据 `assert_swap(delta_cost, t)`：
  - 若 `delta_cost <= 0` 接受；否则以 `exp(-delta_cost/t)` 与随机数比较。
- 单次尝试 `try_swap`：
  - `floorplan->swap_insts(...)` 产生候选位置互换（受 rlim 限制）；
  - 计算成本增量：
    - `delta_bb_cost`：总线 bounding box 变化；
    - `delta_tcost`：时序成本（仅在 TD 模式，`compute_delta_tcost`）；
    - 综合代价：`(1-α)*ΔBB*inv_pre_bb + α*ΔTiming*inv_pre_t`，其中 α=`PLACER::TIMING_TRADE_OFF`。
  - 若接受则更新累计成本并调用 `maintain` 固化更改，否则回滚。
- 温度与半径更新：
  - 初始温度 `starting_t`：以最高温度多次试探，估计方差，设 `20*std_dev`。
  - 退火 `update_t`：基于成功率区间调节（>0.96、>0.8、>0.15…）。
  - 移动半径 `update_rlim`：按成功率线性调节并裁剪到 `[1, max_rlim]`。
- 终止条件 `exit_sa_alg`: `t < 0.005 * cost / num_nets`。
- 时序驱动：
  - 周期性重计算/增量更新 tcost（`TEngine::REBUILD/INCREMENT`），根据 rlim 推导 `crit_exp`，强化关键路径约束。

### 与 VTR 对比（布局阶段）

- FDE 布局为 SA 框架 + 自定义代价（BB + STA 驱动），结构上与 VPR 的经典 SA 相似。
- VTR/VPR 的布局包含更丰富的 cost 组成（拥塞预测、延迟模型、多级放置、静态/动态便宜），并有多线程/并行/更复杂温控与合法性检查；FDE 的实现更轻量，接口面向自研 `Floorplan` 与 `TEngine`。

---

## 路由（`router/`）

### RRG 构建（`common/rrg/`）

- `RRGraph::build_rrg()` 四步：
  1) `init_grm_info`：初始化每列的 GRM/GSB 资源与 LUT 索引；
  2) `distinguish_nets_in_a_column`：在列内区分并为每个 tile 侧端口分配 `PtrToRRGNodePtr`（指向 RRGNode* 的指针槽），在相邻行复制 TOP/BOTTOM；
  3) `spread_nodes_from_last_column`：跨列传播节点指针（支持左右方向宏）；
  4) `build_grms_in_a_column`：真正创建 RRGNode/边 RRGSwitch，记录电阻/电容、边类型（BUF/PT/DUMMY），并统计 pips。
- 查询接口：
  - `find_logic_pin_node(inst,pin,pos)`：从版图位置回查逻辑 pin 对应的 RRG 节点；
  - `find_grm_net_node(net_name,pos)`：定位某 GRM 网的节点。

### 路由算法（`router/src/`）

- 两套：
  - Breadth-First / Directed 搜索（`RouteBreadthFirst.cpp`）：
    - 每轮对所有 net 执行 BFS/定向搜索，heap 扩展邻居，并用拥塞成本更新；失败重试并逐步提高拥塞惩罚 `pres_fac` 与历史代价 `hist_fac`（Pathfinder 风格）。
  - Timing-Driven（`RouteTiming.cpp`）：
    - 先执行 STA，基于每个 sink pin 的 Slack/Crit 计算关键程度；
    - 对 sink 按 criticality 排序，构造 `TDHeap`（A*），逐个目标扩展直至到达；
    - 路径回溯、更新 net 的拥塞成本，再次 STA 增量更新最大延迟 `Dmax`，迭代直到可行或达到最大轮数。
- 共同框架：
  - pres_fac 初始与乘子、hist_fac（历史拥塞）都与 Pathfinder 思想一致；
  - `infeasible_route()` 检查是否所有网已无拥塞，`save_path()` 固化路径。

### 与 VTR 对比（路由阶段）

- 框架相似：VPR 使用 Pathfinder 路由器，支持 BFS/A* 与 timing-driven，逐轮提升拥塞惩罚。
- FDE 的差异：
  - RRG 的构建直接依赖架构库（GRM/GSB/ArchPath）和版图坐标，面向特定 FPGA 结构；
  - 时序驱动路由在 heap 扩展代价中显式使用 pin criticality 与 A* 参数 `astar_fac`；
  - 细节如电容/电阻累加、PT/BUF 类型处理在 `RRGSwitch` 层体现；
  - VTR 的 RRG 通用性更强，支持更复杂开关/延迟模型与大量架构特性；FDE 的实现更贴近具体器件库。

---

## 时序分析（`sta/` 与 `common/tengine/`）

- `TEngine` 提供通用时序图与节点/边数据结构、域（时钟域）管理、拓扑排序与到达/需求时间计算、Slack 更新（`update_slack_to_pin`）。
- `STAEngine` 在项目中复用 `TEngine`：
  - `timing_analysis()`：计算路由延迟、建立时序图、加载延迟、识别顺序元件与主输入/输出、按拓扑计算到达时间并回写到 pin。
  - `mark_sequentials()`：识别含 CLOCK 类型引脚的 primitive 为 DFF，并按 `find_clock_nets` 将不同时钟域映射到 `_clk_nets`；
  - `update_tarrs_to_pin`：将节点的到达时间写入 `PIN` 的 `timingpoint` 属性，用于后续路由/布局。
- `router/timing_driven` 使用 `TEngine::REBUILD/INCREMENT` 接口每轮更新最大延迟与每个 sink 的 Slack。

### 与 VTR 对比（时序阶段）

- FDE 时序引擎内建且与路由/布局交互紧密，延迟模型来自 RRGSwitch/RRGNode 的电阻/电容；
- VTR 有成熟的 TimingGraph/STA 引擎，支持更复杂的时序模式（上升/下降、setup/hold、多时钟、I/O 时序）与 LUT/开关延迟建模；FDE 当前实现相对简洁，更多细节在架构库中硬编码。

---

## 打包（`packing/`）、比特流（`bitgen/`）、其他

- `packing/` 目录存在但未展开源码细节，推测包含：
  - 将 LUT/FF/MACRO 聚合到物理 Slice/CLB；
  - 约束与合法性检查。
- `bitgen/` 提供比特流生成工具链，未在本次课堂文档中展开。
- `viewer/` 提供架构与用户设计可视化，辅助教学展示。

---

## 关键差异小结（与 VTR）

- 工程组织：FDE 自研全栈（对象模型、AIG、映射、RRG、路由、STA），VTR 采用通用 RRG/路由/布局框架并调用 ABC 做映射。
- 架构绑定：FDE 的 RRG/开关类型/PT/BUF 等细节面向特定架构库；VTR 的架构描述更通用（XML Arch），可描述大量器件与开关模型。
- 映射算法：FDE 实现了 K-LUT 映射的割枚举+简化代价（level, area），并本地合成真值表；VTR/ABC 提供更先进的割优化与逻辑重写。
- 布局与路由：两者均为 SA + Pathfinder 思路，但 VTR 的实现更成熟与参数化，FDE 更紧密结合自有 STA 与架构。
- 时序：FDE 的 STA 与 router/placer 紧耦合、接口简单；VTR 的 TimingGraph 更完备、可选复杂模式与优化。

---

## 附：课堂讲解建议

- 建议以“数据流视角”展示：网表读取 → AIG 化 → 割枚举/选割 → LUT 注入 → 打包 → 布局（SA）→ 路由（Pathfinder）→ STA 迭代 → 输出与报表。
- 在映射部分，可现场展示 `calcTruthtable` 的小例子（2 输入合成成 3/4 输入 LUT 的过程），解释索引映射与 truth table 拼接。
- 路由部分，强调 `pres_fac` 与 `hist_fac` 的作用，以及 timing-driven 中 criticality 对 heap 扩展代价的影响。
- 若时间允许，对比 VTR 的同名阶段与接口，说明为什么 FDE 选择自研而非直接集成 ABC/VPR。

---

## 参考源码定位

- 映射：`mapping/mapping.cpp`, `mapping/aigtransform.cpp`, `mapping/patternmap.cpp`
- 布局：`placer/plc_algorithm.cpp` 及 `plc_*` 系列
- RRG：`common/rrg/rrg.hpp`, `common/rrg/rrg.cpp`
- 路由：`router/src/RouteTiming.cpp`, `router/src/RouteBreadthFirst.cpp`
- 时序：`common/tengine/tengine.hpp`, `sta/sta_engine.cpp`
