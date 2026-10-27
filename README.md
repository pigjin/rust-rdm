# RedisDock

RedisDock 是基于 [Tiny RDM](https://github.com/tiny-craft/tiny-rdm) 使用 Rust 重构的原生 Redis 桌面管理客户端，采用 GPUI 构建图形界面。

项目延续 Tiny RDM 的使用体验，以 Rust 实现 Redis 连接、数据操作和桌面交互，面向日常开发、调试与运维场景。你可以在同一个窗口中管理多个连接、浏览和编辑数据、执行命令，并查看 Redis 的运行状态。

## 主要功能

- **连接管理**：管理多个 Redis 连接与分组，支持 TCP、Unix socket、Sentinel 和 Cluster。
- **安全连接**：支持 TLS、双向 TLS、SSH 隧道和代理；SSH 支持密码、私钥与 agent 认证，并校验和保存服务器主机指纹。
- **数据浏览与编辑**：支持 String、Hash、List、Set、ZSet、Stream 和 RedisJSON，提供键筛选、集合分页、内容编辑、重命名、TTL 设置和批量删除。
- **命令行**：内置 Redis 命令终端，支持命令执行和历史记录。
- **运行状态**：查看服务器信息、客户端连接、实时监控、慢日志和命令运行日志。
- **发布与订阅**：订阅频道、查看消息并发布内容。
- **导入与导出**：支持 Redis 数据的 CSV 导入导出，以及兼容 Tiny RDM 格式的连接 ZIP 备份。
- **界面与显示**：支持浅色、深色主题和多语言，可调整字体，使用内容格式化、解码及自定义转码器。

RedisJSON 需要 Redis 服务端提供对应模块；PHP 转码需要本机 PHP，Pickle 的 datetime 兼容转换需要 Python。

## 下载安装

前往 [GitHub Releases](https://github.com/pigjin/rust-rdm/releases)，选择对应系统和处理器架构的安装包。

| 系统 | 架构 | 安装方式 |
| --- | --- | --- |
| macOS | `arm64`（Apple Silicon） / `x86_64`（Intel） | 下载 `.dmg`，打开后将 RedisDock 拖入 Applications |
| Windows | `arm64` / `x86_64` | 下载对应的 `-setup.exe` 并运行安装器 |
| Linux | `arm64` / `x86_64` | 下载 `.AppImage`，执行 `chmod +x <文件名>.AppImage` 后运行 |
| Debian / Ubuntu | `arm64` / `x86_64` | 下载 `.deb`，使用 `sudo apt install ./<文件名>.deb` 安装 |

安装包名称包含版本和架构，例如 `RedisDock-<版本>-macos-arm64.dmg`。DEB 包内部使用的架构名称分别为 `arm64` 和 `amd64`。

使用前请留意：

- macOS 安装包使用本地签名，尚未接入 Apple 公证；Windows 安装包未签名，首次安装可能出现系统安全提示。
- Windows ARM64 安装器通过系统兼容层启动，安装的是原生 ARM64 应用。
- Linux 安装包以 Ubuntu 24.04 为构建基线，支持 X11 和 Wayland，需要系统图形驱动与 Vulkan 支持，不保证兼容更旧的 glibc。
- 各平台与架构的构建、打包支持不代表已经完成完整 UI 和真实硬件验收。

发布页提供 `SHA256SUMS`，可用于核对安装包的 SHA-256 校验值。

## 源码运行

当前 CI 使用 Rust 1.99.0。请先准备 Rust 工具链和对应平台的编译环境：macOS 可使用 Command Line Tools；Windows 需要 MSVC、Windows SDK 和可用于构建 OpenSSL 的 Perl；Linux 的系统依赖可参考 [发布工作流](.github/workflows/release.yml)。

```sh
git clone https://github.com/pigjin/rust-rdm.git
cd rust-rdm
cargo run --locked
```

编译 release 版本：

```sh
cargo build --locked --release
```

默认输出为 `target/release/redisdock`，Windows 下为 `target/release/redisdock.exe`。

### macOS 应用包

打包需要完整 Xcode 26 或更新版本，以及 Python 3。通过 `DEVELOPER_DIR` 或 `xcode-select` 选择 Xcode 后，在项目根目录运行：

```sh
scripts/bundle-macos.sh release
python3 scripts/check-macos-bundle.py release
```

应用包输出为 `target/bundle/RedisDock.app`。图标、图片和语言资源已嵌入程序，运行时不依赖源码目录。

## 致谢与许可

感谢 [Tiny RDM](https://github.com/tiny-craft/tiny-rdm) 项目及其贡献者。RedisDock 基于该项目使用 Rust 重构，沿用其图标、图片和语言资源，并保留相应的来源与许可声明。

本项目沿用原项目的 **GPL-3.0-only** 许可，详见 [LICENSE](LICENSE)。第三方组件和资源的许可及来源说明见 [assets/licenses](assets/licenses/) 与 [vendor](vendor/) 中的许可和修改声明。
