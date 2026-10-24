# 安全与性能回归说明

本次工作基于 `a97605d`，在 `codex/fix-safety-and-performance` 分支执行，不提交、推送或发布。

## 批准改变的行为

- Hash、Set、ZSet 替换先验证并写入新值，再删除旧值；同值不删除，保留键的剩余 TTL。删除失败返回部分完成错误。连接中断不重放结果未知的写操作，不增加 EVAL 权限要求。
- 连接 ZIP 必须含 `connections.yaml` 且能解析为连接列表，之后才原子覆盖。合法备份保留原始字节、未知字段和既有兼容规则；导入不套用连接表单校验。
- CSV 导出取消或失败会丢弃临时文件，保留已有目标；界面分别显示取消、失败或完成。
- SSH 在握手后、发送密码/私钥认证/agent 请求前校验主机。未知主机需用户确认算法和 SHA-256 指纹；同端点密钥变化拒绝连接。信任记录绑定主机与端口。保存的连接确认后原子保存；未保存表单只更新草稿，随表单保存。
- `Service::detail` 保留参数并返回一个扫描批次，`KeyDetail::stream_cursor` 独立记录 Stream 的实际扫描位置。UI 管理继续、加载全部和取消。List 保留物理索引，扫描无匹配批次也能继续。
- 监控与订阅各保留最近 10,000 条、最多 32 MiB 文本，以先达到的限制为准。单条超预算不保留；显示保留与淘汰数量，复制、筛选、导出仅针对保留记录。清空重置计数，停止再启动保留历史。待消费队列同样有大小上限，突发超量计入淘汰。
- 键树可见行缓存按键数据版本、过滤、展开、平铺、分隔符等状态失效，继续使用虚拟列表。

## 原版取证与独立预期

`tests/fixtures` 中的原版样本保持不变。本次批准修复的危险行为在 `tests/core.rs` 的兼容测试中明确列出独立预期，不改写原始样本。新增故障测试覆盖 ACL 拒绝、部分完成、唯一成员 TTL、非法编码、取消/写盘/解压失败与旧文件保留。协议测试覆盖分片、超时续读、嵌套、流水线及处理量线性增长。

UI 原有 556 个注册场景保留，新加 4 个 `safety-` 场景；视觉负例与阈值不放宽。离屏验收、核心网络测试与原生窗口验收分别记录，不互相替代。

## 回归入口

```sh
go -C tests/fixtures/ssh-server build -o /tmp/redisdock-ssh-server .
REDISDOCK_SSH_FIXTURE=/tmp/redisdock-ssh-server \
REDISDOCK_JSON_MODULE=/path/to/rejson.so \
cargo test --locked -p redisdock --no-default-features --lib --tests -- --include-ignored
cargo test --locked -p redisdock --lib
cargo build --locked -p redisdock --features ui-qa --example render_qa --example seed_demo
python3 scripts/run-ui-qa.py --output outputs/new-full-regression
cargo test --locked -p gpui-base --features test-support rdm_
cargo test --locked -p gpui-base --features test-support --lib hover_card::tests
cargo test --locked -p gpui-pre-apple --features font-kit,runtime_shaders,test-support --lib text_system::tests
python3 scripts/test-release.py
scripts/bundle-macos.sh debug
python3 scripts/check-macos-bundle.py debug
```

PR 与 main 分支新增 macOS 隔离 Redis 核心/网络回归，发布的跨平台构建依赖该检查。线性性能使用处理量断言，不给 CI 设置浮动耗时门槛。实际运行的计数、五次性能数据、原生截图和未执行项以本轮自包含 HTML 报告为准；配置工作流不表示已在 GitHub 执行。
