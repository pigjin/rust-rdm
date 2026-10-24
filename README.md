# RedisDock

RedisDock 是使用 Rust + GPUI 构建的原生 Redis 桌面管理客户端，基于 Tiny RDM 迁移。原项目参考目录为
`/Users/zlsj/Projects/tiny-rdm`；Web 和 Docker 版本不在本次范围内。

更名验证（2026-10-22）：30 项库测试、61 项核心集成测试和 9 项网络测试通过，覆盖旧配置目录与环境变量兼容。
当前注册的 554 个 UI 场景中，本轮执行 6 个相关场景；另用隔离配置核对实际窗口、欢迎文案、关于页和应用/退出菜单的 RedisDock 名称。
`RedisDock.app` 的名称、应用标识、严格签名及 12 个代码/常量段检查通过，测试进程已退出，默认用户配置哈希不变。
本轮未重新执行全量 UI 或 Windows/Linux 验收；以下完整 macOS 验收记录对应更名前的版本。

macOS 桌面版已覆盖 8 大核心服务（连接、浏览器、命令行、慢日志、监控、发布/订阅、运行日志、偏好设置）及连接、分组、偏好与解码器、新建键、添加/编辑行、重命名、TTL、批量删除、清空数据库、CSV 导入导出和确认警告框。

更名前验收复核（2026-10-22）：原始 512 个 UI 场景先完整跑通；修复后保留原场景及顺序，新增 40 个关键页面对照状态、1 个实际操作回归和 2 个监控长行回归，最终 **555/555** 场景通过。重新执行的 **120/120** 项相关测试、macOS 应用包严格签名和 12 个代码/常量段一致性检查通过。

原版 macOS WKWebView 的 40 个明暗主题关键状态通过 **1558 项布局 + 838 项绘制** 检查。实际监控启停与重启、发布订阅、空白频道、日志筛选与三态排序、TTL 快捷设置、改名、批量删除和带确认门槛的清库均有当前回归。最终签名包另完成 16 项实际窗口验收，覆盖中文保存、TTL、命令行、监控、原生保存面板、发布订阅、明暗主题、Hash、RedisJSON、日志确认及 Cmd+Q 退出。离屏与原生 Redis 夹具均恢复并退出，默认用户配置哈希不变。

**更名前的 macOS 功能与关键页面视觉验收通过。** 实际窗口发现的监控长行重叠、键盘与辅助功能激活后的提示框残留均已修复并回归，禁用按钮的操作限制保持有效。旧 Chrome 的 Unicode/Emoji、五项底栏宽度及 43 项图表实色描边掩膜差异继续保留；未宣告 Windows/Linux 或整图逐像素一致性。

完整证据与可离线打开的对照报告见 [macOS 验收报告](outputs/macos-acceptance-20261021/index.html)。以下为此前各阶段记录；本轮状态以上述结果为准。
实现范围、已有证据和说明见 `docs/parity.md`。

客户端弹层进展（2026-10-21）：三态排序、重开恢复默认连接时长降序、系统字体、等宽列和窄窗口定位，
通过 11 组实际 GPUI 场景的 1908 项尺寸/数据检查与 915 项限定绘制检查。
新增 6 组长地址、滚动和加载场景也已接入，共注册 512 个验收场景；新增状态通过 1530 项布局、558 项绘制及
128 项断行、滚轮、真实请求等待/完成和静态加载图标检查。刷新保留旧行，浅色/深色分别按原版 0.5/0.38 淡化并阻挡表格操作。
完整弹层裁片仍不同，动画时序、滚动条绘制、错误状态及原生窗口仍待验收。
本轮构建与完整回归进度见 `outputs/status-clients-extended-runtime-progress.json`。

状态页进展（2026-10-21）：统计卡片、24px 数值和独立单位、页签与原生图表已按原版 macOS 样本修正。
12 组图表样本涵盖明暗主题、不同字体、零值、空图、Valkey、波动、缺失采样及大数值；
原版的千位分隔和科学计数刻度、整数零值坐标、图例与曲线几何已核对。
状态信息页的页签右侧筛选、纵向分组、空分组回退和带边框条纹表格通过 6 组样本、829 项尺寸/数据检查；
鼠标切换、清空后焦点和页签保留另有实际 GPUI 事件回放。222 项限定绘制检查通过，六组完整卡片裁片仍未逐像素一致。
页签的 4 项字体宽度差异已修复：59 个实际 GPUI 场景回放完成，1229 项状态布局、248 项大数值布局、
2777 项图表几何及 829 项信息表格检查通过；711 项字体和 222 项信息表格限定绘制检查继续通过。
修复保留请求的 CSS 字重，并用公开 CoreText 家族查询返回的来源重建同一命名字形，未固定字体路径或补偿像素宽度。
原生文字系统的 72 个原版字宽样本及原有 8 项测试，在沙盒和实际应用权限环境均通过；此前环境差异导致的失败记录保留。
43 项图表实色描边掩膜和完整裁片差异仍存在，完整功能、视觉及原生验收继续未完成。
上一状态页应用包已完成 System14 / Courier20、两种主题和 960px 最小宽度的限定原生检查，并核对退出和测试服务清理；
该原生回放早于信息页、大数值与字体修复。字体修复构建的完整回归记录见 `outputs/local-fallback-runtime-progress.json`，
其状态页离线对照见 `outputs/local-fallback-review.html`；此前 450 对总对照页保留，浏览器交互均未复验。

上一阶段集合筛选控件回归（2026-10-20）：477 个 GPUI 场景完成，必需的模型与尺寸断言通过，该轮新跑 27 项库测试。
此前 19 项基础编辑/Apple 文字测试按 482 个未变输入沿用，另有 61 项核心和 9 项网络结果按 118 个输入沿用；共 89 项保留结果，加上本轮 27 项为 116 项证据，不是全部新跑。
集合表格已按原版 macOS 的 14px 字体、34px 表头、33px 普通行和数字等宽排版修正；
24 个状态及两种主题的删除确认框通过限定尺寸检查，表格另有 474 项背景与文字边界检查。
筛选框宽度改为读取公开原生字体指标，120 个独立 WKWebView 输入控件样本通过；
涵盖八种字体、三种字号和五种列数。空值仍预留清除按钮位置，窄窗口允许按原版收缩。
旧 Chrome 表格和删除框差异仍保留。左侧键筛选另通过 22 个原版操作/尺寸状态及 174 项限定图像检查；
文字固定为 14px，清除按钮预留 16px，明暗背景与焦点环按原版修正，工具按钮采用实际指定图标尺寸。
匹配标签和禁用搜索另有六个原版样本、90 项限定图像检查：标签字号为 12px、圆角为 4px，
关闭命中框为 12px、SVG 为 16px；禁用搜索按钮的透明度在浅色为 0.5、深色为 0.38。
实际渲染 Click 动作和 12 次有输入时的鼠标搜索通过；GPUI 尚未暴露原版禁用辅助功能标记。
默认 300px 侧栏下的长标签按原版直接裁切，标签宽度上限为内层可用宽度的一半；
六个场景、96 项限定图像检查通过；旧离屏脚本仅发送双击第二次按下事件，原生完整双击的焦点差异另行保留。
侧栏拖动现按原版最小 300px、横坐标减 50px 计算，移除了旧 700px 上限；
16 个明暗场景通过实际 GPUI 左键拖动，包含 800px 和超过窗口宽度的 1200px。
4px 分隔条的稳定悬停、按下和拖动颜色及 1 秒尾沿保存已核对，294 项限定图像检查通过。
类型标签按 14px/500 字重和实际字宽布局，保留小数宽度到布局结束；700px 下最后一个裁切字符已核对。
中间动画帧、更多字体/类型/鼠标键、集合匹配状态、光标、IME、辅助功能和原生窗口仍待继续核对。
七种类型及全部类型、自定义和缺失字体、中文/Emoji 另有 28 个实际 GPUI 状态通过尺寸和模型检查。
macOS 上，非空配置字体栈全部不可用时按原版使用 Times；空栈使用系统字体。
字体首选项现按实际可加载家族判断，补足 macOS 枚举遗漏的 Courier、Times；
通用字体回退列表规范化可用名称并跳过缺失名称，主字体选择也用于编辑器及终端。
类型标签边框已按主题修正；浅色输入框使用原版边框色，深色未聚焦边框透明。
当前 711 项字体/标签限定图像比较通过；原五项文字失败已修正，原始失败记录和阈值保留。
三种传统系统字体按原版引擎修正上升高度；共享匹配标签使用 CSS 基线取整，保留普通文字与父级命中/裁切范围。
49 组独立 WebKit 字体框样本与 8 组原版组件基线提供依据；字体平滑和光栅化实现未修改。
JSON 着色新增 36 组原版 Monaco 样本，覆盖注释、字符串角色及跨行状态；
四个编辑场景通过 24 项限定颜色检查。连接标签的九个限定布局通过尺寸和 88 项颜色检查，
活动文字使用 800 字重；切换连接保留各自的行编辑器、草稿及全屏状态，隐藏转换和保存回调更新所属编辑器。
连接标签关闭按钮和 Cmd+W 按原版先显示确认框，取消和 Escape 保留编辑器及草稿。
确认后等待连接关闭再移除标签，活动索引保留原版的处理顺序；五种多标签状态及
目标 Redis 连接池已核对。两种主题的确认框通过尺寸及 18 项限定颜色检查。
GPUI 已通过 497 条原版字体宽度检查，涵盖普通及等宽字体、九种系统字重和混合字重。
Apple 后端保留完整原生字体实例、逻辑光学字号及中文 Text 字体资源；修改和负对照另有记录。
底栏另通过 40 条 macOS 字体宽度及 17 个场景检查：通用字号为 14px、20px 时，
原版按钮与菜单均保持 14px，新版已据此修正。JSON 诊断覆盖 77 组原版模型输入，选择/复制/焦点恢复有 9 个
限定场景，长提示滚动有 4 个限定场景。另有三个诊断文字焦点的保存场景；
Cmd+S 按原版提交所属值编辑器，已通过隔离 Redis 核对并恢复数据。
诊断正文、跨消息及底部文字的键盘行为共匹配 263 个原版状态，49 次实际复制
核对后逐次恢复剪贴板；覆盖字符、单词、中文分词、换行、Tab、错误代码、
底部滚动条的不可见光标停留，以及选区缩空后的停留。Cmd+K Cmd+I 首次显示
提示时保留编辑焦点，再次执行进入提示；Escape 返回编辑器。四项限定底部
选区范围检查通过，不能据此认定所有颜色或整图一致。
14 组严格视觉脚本中 12 组通过；Unicode 选区和 Emoji 标记与暂用的 Chrome
参考图仍各有三个物理像素的范围差异，两像素阈值未放宽。macOS 原版引擎的
字体宽度与 Chrome 存在差异，没有用字体补偿把这些失败比较改为通过。
底栏另保留 5 项 20px 字体宽度差异；旧 Chrome 样本不变，新增的 macOS 样本独立检查。
另用原版 WKWebView 页面的一个 Emoji 场景核对下划线和右侧问题标记，两项
范围检查通过；这项结果仅覆盖标记范围，整图一致性仍待验收。
450 对参考图已完成离线图片完整性检查，保留此前 426 对，新增 24 对匹配提示及有、无草稿的集合输入原始像素裁片；
该对照页的浏览器交互尚未复验。集合控件阶段的 3051 个构建、测试及原版取证输入已核对；此前 2783、2494 和 2480 个输入的检查记录保留。
此前 344 场景构建完成过一项原生提示焦点、
底部扩选和所属 Hash 字段保存回放，测试值已恢复，程序退出已独立确认。
当前应用包已用隔离配置完成部分原生窗口回放：Courier、中文/Emoji 条件和窄侧栏显示已检查，退出和数据隔离审计通过。
随后已修正原生匹配标签失焦：原版实际窗口确认预期行为，当前包的 ASCII、中文/Emoji 及 Hash 筛选可直接回车重新匹配。
上阶段 8 个完整双击离屏回归和完整 452 场景记录已保存；集合控件的 477 场景完成必需断言，711 项字体/标签限定图像检查通过，原生整体验收仍未完成。
原版 Wails 的限定实际窗口对照另有记录。其余键盘边界、WKWebView 整图、更多字体和主题、
其余原生操作及 Windows/Linux 验收仍待完成。
集合表格的大字号字体和行高已按原版 macOS 修正，更多控件状态仍待验收。
集合控件阶段的完整范围见 `outputs/entry-controls-runtime-check.json`，该阶段原生限定回放见 `outputs/entry-controls-native-observations.json`。
状态页时长、精确半数舍入、去尾零、网络采样与五点历史已按原版修正，32 组实际渲染数值通过。
匹配提示按原版保持 14px 字体，集合模式显示星号；100ms 显示/隐藏、移入正文保持、右侧/底部移出及短条件居中已核对。
12 组集合输入宽度覆盖 System/Courier、1024/1340 窗口和短长条件；20 组提示通过 232 项限定绘制检查，其中新增 32 项箭头周围固定阴影采样。
当前真实窗口已检查长 ASCII、中文/Emoji 的键和集合提示，以及双击后回车、清除后恢复。退出、签名、12 段代码、19 个测试键和配置隔离审计通过。
完整回放日志保留 5 项旧 Chrome 底栏宽度差异，原始失败结果未改；必需执行情况另见 `outputs/entry-controls-full-execution-audit.json`。
集合输入和搜索按钮已修正连续边框、圆角及禁用配色，176 项严格颜色检查通过；原 84 项失败和各中间阶段均保留。当前报告见 `outputs/entry-controls-visible-full-image-check.json` 与 `outputs/entry-controls-hidden-full-image-check.json`。
状态页最新图表和信息页记录见本页上方进度；完整阴影与动画、辅助功能及其他原生操作仍需继续对齐。

## 构建和运行

```sh
cargo run --locked
```

已在 Apple Silicon macOS、Rust 1.99 环境构建。GPUI 通过 Metal 渲染，
已启用运行时着色器编译，可使用 macOS Command Line Tools。

生成 macOS 应用包需要完整 Xcode 26 或更新版本，使用其 `actool` 编译原生应用图标
（仅 Command Line Tools 仍可运行 `cargo run`）。通过 `DEVELOPER_DIR` 或 `xcode-select` 选择 Xcode 后运行：

```sh
scripts/bundle-macos.sh debug
# 或 scripts/bundle-macos.sh release
python3 scripts/check-macos-bundle.py debug
# release 构建的核验命令也使用 release
```

输出为 `target/bundle/RedisDock.app`。构建脚本使用本地签名，包含静态 OpenSSL 和第三方许可。
图标、图片和语言资源在 debug 与 release 构建中均嵌入程序，不依赖源码目录。
macOS 原生图标源文件为 `assets/images/AppIcon.icon`，可用 Icon Composer 编辑。
应用包默认采用亮色图标；macOS 26+ 的 Finder 和 Dock 遵循系统的“图标与小组件样式”，
选择“深色 → 自动”时随系统深浅色切换，不受应用内主题设置影响。旧系统与直接运行二进制时，
运行中的 Dock 图标跟随系统深浅色，未运行时使用亮色兼容图标。
当前应用包链接最低版本为 macOS 11，已验证的运行环境为 macOS 26。

## GitHub Releases 发布

仅推送 `v` 开头的 tag 会触发 GitHub Actions；普通提交与 PR 不触发构建。
发布前更新 `Cargo.toml` 的版本，并运行 `cargo check --locked`；若修改版本后锁文件需要同步，
先运行 `cargo check` 更新 `Cargo.lock`，将两者与工作流、打包脚本一起提交后再创建 tag：

```sh
# 当前源码版本为 0.1.1；后续将下面的版本替换为 Cargo.toml 中的版本。
git tag -a v0.1.1 -m 'RedisDock 0.1.1'
git push origin v0.1.1
```

tag 去掉 `v` 后必须与源码版本完全一致，例如 `v0.1.1` 发布为 **RedisDock 0.1.1**。
`v0.2.0-rc.1` 等预发布版本同样要求源码版本一致，并标记为 Prerelease。
工作流固定 Rust 1.99.0，六种目标均执行无桌面库测试、release 构建与安装包检查。
所有目标成功后才创建 Release 草稿，上传八个安装包和 `SHA256SUMS`，核对上传完整后自动发布。
失败的草稿可通过重跑工作流补齐；已经发布的同名 tag 会拒绝覆盖。
使用内置 `GITHUB_TOKEN`，仅发布作业拥有 `contents: write`，无需额外配置 PAT 或签名 Secrets。

| 系统 | 架构与下载文件 | 安装方式 |
| --- | --- | --- |
| macOS | `RedisDock-<版本>-macos-arm64.dmg` / `x86_64.dmg` | Apple Silicon 选择 arm64，Intel Mac 选择 x86_64；打开 DMG，将 RedisDock 拖入 Applications |
| Windows | `RedisDock-<版本>-windows-arm64-setup.exe` / `x86_64-setup.exe` | 选择对应架构并运行；安装到当前用户的 Programs 目录，提供开始菜单和卸载入口，卸载保留用户配置 |
| Linux | `RedisDock-<版本>-linux-arm64.AppImage` / `x86_64.AppImage` | `chmod +x` 后运行；AppImage 内嵌用户态依赖，仍需要系统图形驱动和 Vulkan 支持 |
| Linux | `RedisDock-<版本>-linux-arm64.deb` / `x86_64.deb` | 使用 `sudo apt install ./RedisDock-<版本>-linux-<架构>.deb` 安装；DEB 内部架构分别为 arm64 / amd64 |

Linux 以 Ubuntu 24.04 为构建基线，启用 X11 和 Wayland；不承诺兼容更旧的 glibc。
Windows ARM64 安装器通过 Windows 兼容层启动，安装的是原生 ARM64 应用。
macOS 使用本地签名，未接入 Apple 公证；Windows 安装包未签名，首次安装可能出现系统安全提示。
这些 CI 检查覆盖构建与打包，不代表 Linux、Windows 或各架构已经完成完整 UI 和真实硬件验收。

Linux/macOS 下载后可用 `sha256sum -c SHA256SUMS`（macOS 用 `shasum -a 256 -c SHA256SUMS`）核对完整下载集。
仅下载单个安装包时，核对清单中对应文件的一行即可；Windows 可使用 `Get-FileHash -Algorithm SHA256 <文件>` 对照。
本机发布逻辑检查命令为 `python3 scripts/test-release.py`；macOS DMG 可用 `scripts/package-macos.sh arm64` 构建。

## 配置和兼容

macOS 新安装默认配置目录为 `~/Library/Application Support/RedisDock`。
也可用 `REDISDOCK_DATA_DIR` 指定配置目录，用 `REDISDOCK_LANGUAGE` 指定自动语言。
若新目录不存在且旧版 `~/Library/Application Support/rust-rdm` 目录存在，会继续使用旧目录，
保留已有连接与偏好，不移动或复制用户配置。旧环境变量 `RUST_RDM_DATA_DIR`、
`RUST_RDM_LANGUAGE` 仍可使用，新变量优先。

已移除更新检查、赞助、Twitter 以及 Tiny RDM 网站跳转。项目源码与问题反馈使用 GitHub 链接，
编辑器内容链接和 Redis、SSH、代理功能保留。旧配置中的更新选项加载时被忽略，不再写入保存后的配置。

连接和偏好设置使用原项目兼容的 YAML 字段结构。连接列表的更多菜单使用
原版 ZIP 备份格式，包含 `connections.yaml`；导入须找到 `connections.yaml` 并成功解析为连接列表后才原子覆盖，保留合法备份的原始
YAML 字节；缺失、损坏或无效 YAML 不改动已有配置。配置加载及打开编辑表单时的默认值处理分别遵循原版后端和前端。
Redis 数据导入、导出兼容原版 CSV：十六进制键名、DUMP 数据及
可选的绝对过期毫秒时间。提交后表单立即关闭，可继续浏览；关闭底部进度消息
会取消对应任务。导入可选择完成后重新载入键列表。
CSV 读取按原版保留 BOM，严格检查引号和列数，在首个语法错误处停止；
非法十六进制行跳过后继续读取。
批量勾选保留选择顺序；导出的受影响键列表支持拖选、跨行选择及复制。
长键列表支持滚轮和滚动条拖动。
自定义 TTL 的单位支持方向键选择、回车或空格确认，取消后重开保留单位。
CSV 导入、导出表单和 TTL 单位菜单跟随浅色、深色主题。
macOS 原生文件面板按原版附着主窗口，提供 ZIP/CSV/TXT 过滤及对应默认文件名；
证书和自定义解码器文件保留任意后缀。取消普通文件选择会清空表单路径。

新建键弹窗包含数据库筛选、七种类型、秒数 TTL 与永久按钮、按类型切换的普通文本框和动态行，
以及携带所选数据库的导入入口。文本框可调整高度；创建失败时保留表单。
空行、集合去重、JSON 默认值和已有键的 TTL 处理按原版限定样本对照。
五类添加集合元素弹窗支持覆盖/忽略或追加/前置，并记忆重新载入勾选；
默认更新当前页及元信息，勾选后重新读取完整详情。
Hash、List、Set、ZSet 行编辑按原版响应局部更新当前页和摘要，保留分页位置；
未固定时点击更新即关闭面板，固定状态和空值回退按原版分别处理。
编辑面板支持固定、全屏与恢复；Cmd+S 在值编辑器或其诊断文字获得焦点时提交，
未修改内容也会按原版发出请求。字段输入框中的同一快捷键不提交。
格式和解码菜单显示标题、选中状态与自定义分组，支持方向键、回车和 Escape；
关闭菜单保留编辑面板，解码器设置可返回原编辑内容。
打开解码器设置时继续显示父编辑面板；取消保留草稿和全屏并恢复主题，
保存保留草稿和全屏，并同步应用编辑器开关。诊断文字上的 Cmd+S 转交给所属的当前值编辑器。
首次建立行编辑面板时会从原值转换。非空原值切换格式时，等待期间清空值显示，
仍可输入和保存；返回值更新绑定属性时按原版再转换一次。设置弹窗打开期间继续转换，
转换本身不显示全局加载提示。原值为空且显示属性保持空值时，屏幕草稿可继续显示，
转换回调按原版重置待提交内容，编辑器快捷键使用该待提交值。

支持 TCP、Unix socket、TLS、双向 TLS、SSH、Sentinel、Cluster 和 HTTP/HTTPS/
SOCKS5 / SOCKS5H 代理；支持 String、Hash、List、Set、ZSet、Stream 和 RedisJSON。
HTTPS 代理选项按原版发送普通 CONNECT；Redis TLS 仍由 SSL 配置独立控制。
系统 Redis 代理按原版读取 `ALL_PROXY` / `all_proxy` 和 `NO_PROXY` / `no_proxy`。
SSH 支持密码、私钥、agent 及远端 TCP / Unix socket 转发。
首次连接在认证前显示端点和 SHA-256 主机指纹，确认后保存绑定主机与端口的信任记录；
同一端点密钥变化会阻止连接并显示新旧指纹。旧配置首次连接也须确认。
安全与性能调整、原版样本保留规则及回归命令见 [安全与性能回归说明](docs/safety-performance.md)。
RedisJSON 需要服务器提供对应模块。PHP 转码需要可执行的 PHP；Pickle 的
datetime 兼容转换使用 Python。自定义转码器保留原版 Base64 输入输出与
`{VALUE}` 参数占位约定。

## 验证

Redis 集成测试要求 `redis-server`、`redis-cli` 和 `openssl` 可执行。
测试自动启动临时 Redis，使用独立临时目录和回环网络。
SSH 检查另需 OpenSSH 的 `sshd`、`ssh-keygen`、`ssh-agent`、`ssh-add`。
密码、agent 和 Unix socket 的组合检查使用测试专用 Go SSH 服务，需 Go 1.25；
应用运行不使用该服务。RedisJSON 检查另需本机兼容的 RedisJSON 模块，并通过
`REDISDOCK_JSON_MODULE` 指定模块文件。完整检查命令：

```sh
go -C tests/fixtures/ssh-server build -o /tmp/redisdock-ssh-server .
REDISDOCK_SSH_FIXTURE=/tmp/redisdock-ssh-server \
  REDISDOCK_JSON_MODULE=/path/to/rejson.so \
  cargo test --locked -p redisdock --no-default-features --lib --tests -- --include-ignored
cargo test --locked -p redisdock --lib file_dialog_
cargo test --locked -p redisdock --lib csv_messages_match_original_store_samples
cargo test --locked -p redisdock --lib json_brackets_match_original_samples
cargo test --locked -p gpui-base --features test-support rdm_
cargo test --locked -p gpui-base --features test-support --lib hover_card::tests
# macOS Apple 文字后端
cargo test --locked -p gpui-pre-apple --features font-kit,runtime_shaders,test-support --lib text_system::tests
```

离屏界面验收使用程序自己的渲染场景，输出图片，不截取用户桌面：

```sh
mkdir -p /tmp/redisdock-demo
redis-server --port 0 --save '' --appendonly no \
  --unixsocket /tmp/redisdock-demo/redis.sock
# 在另一终端运行（先准备 /tmp/redisdock-demo 目录）
cargo run --locked --no-default-features --example seed_demo -- /tmp/redisdock-demo --render-qa
# 另一个终端：启动带 RedisJSON 的第二个隔离服务。
mkdir -p /tmp/redisdock-json-demo
redis-server --port 0 --save '' --appendonly no \
  --unixsocket /tmp/redisdock-json-demo/redis.sock --loadmodule /path/to/rejson.so
# 再在验收终端准备数据；两个 Redis 服务都保持运行。
cargo run --locked --no-default-features --example seed_demo -- /tmp/redisdock-json-demo --render-qa
REDISDOCK_JSON_QA_DIR=/tmp/redisdock-json-demo \
  cargo run --locked --features ui-qa --example render_qa -- \
  /tmp/redisdock-demo outputs/ui-qa
```

离屏场景要求 `seed_demo` 写入的 `RDM-QA-FIXTURE` 标记。布局对照位于
`outputs/parity-review.html`，具体证据和待完成项目见 `docs/parity.md`。
复制验收会临时使用系统剪贴板；每次复制后、检查结果前立即恢复先前条目，
并核对文本恢复，避免后续场景中断时遗留测试文字。

`src/redis.rs` 是二进制安全的 RESP 传输；`src/service.rs` 实现 Redis 数据操作；
`src/codec.rs` 实现编码和格式转换；`src/ui` 是 GPUI 桌面界面。
CLI 命令解析保留原版 Go `SplitCmd` 规则；字格绘制使用 vt100，并按原版
xterm 的 Unicode 6 表计算字符宽度。字号变化时按原版字符测量和画布取整规则计算行列，
保留原版已有行数与零字号回退行为。Go 和 xterm 输出样本保存在 `tests/fixtures/`。
连接表单按原版顺序校验常规页和高级页，并在输入时更新错误提示；原版表单行为
样本同样保存在 `tests/fixtures/`。
该目录还包含原版 Go 生成的 ZIP，以及配置加载、备份读取和编辑表单合并样本。
原版导出方法清单位于 `docs/source-api-inventory.json`。

## 许可

沿用原项目 GPL-3.0-only 许可，见 `LICENSE`。图标、语言和图片保留原项目资源。
`vendor/gpui-base` 是 Apache-2.0 的 gpui-base 0.7.3，保留其许可文件；本地编辑器
扩展说明见 `vendor/gpui-base/RDM-CHANGES.md`。
`vendor/gpui-apple` 基于 Apache-2.0 的 gpui-pre-apple 0.3.9，加入 macOS 字体
回归测试；生产排版和绘制保持上游实现。修改说明见其 `RDM-CHANGES.md`，
许可及修改声明见 `assets/licenses/GPUI-Apple-LICENSE.txt`，随源码和应用包提供。

`assets/icons/empty.svg` 与 `notice-*.svg` 来源于 Naive UI 的 MIT 许可图标，许可见
`assets/licenses/Naive-UI-LICENSE.txt`；OpenSSL 许可见 `assets/licenses/OpenSSL-LICENSE.txt`。
`vendor/vt100` 基于 MIT 许可的 vt100 0.16.2；字符宽度表来自 MIT 许可的
xterm.js。修改说明及两份许可随源码和应用包提供。
ZIP 依赖许可见 `assets/licenses/ZIP-LICENSE.txt`。
CSV 导入读取器按 Go `encoding/csv` 默认语法实现；Go 的 BSD 许可见
`assets/licenses/go-csv.txt`，随源码和应用包提供。
客户端换行使用 unicode-linebreak 0.1.5 的 Unicode 断行机会及原版浏览器的 URL 规则；
其 Apache-2.0 许可见 `assets/licenses/Unicode-Linebreak-LICENSE.txt`，随应用包提供。
JSON 诊断扫描与恢复规则、波浪线 SVG 来自 Monaco Editor 0.47.0 所含的 MIT
许可代码，原始许可和说明见 `assets/licenses/monaco-json-diagnostics.txt`，随应用包提供。
诊断按钮的 SVG 轮廓提取自该版本的 Codicon 字体；来源、修改说明、CC BY 4.0
内容许可及相关 MIT 代码许可见 `assets/licenses/Codicon-LICENSE.txt`，随应用包提供。
`vendor/gpui-component` 基于 Apache-2.0 的 gpui-component 0.7.3，扩展了原版菜单
样式和紧凑按钮的图标显示，默认组件样式保持不变。修改记录见该目录的
`RDM-CHANGES.md`，许可随源码和应用包提供。

