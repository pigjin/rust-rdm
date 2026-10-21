#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Package existing layout QA images into a standalone comparison viewer."""
import base64
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
NAMES = {
    'entry-diagnostic-hover-scroll-top': '长诊断提示顶部',
    'entry-diagnostic-hover-scroll-middle': '长诊断提示滚动',
    'entry-diagnostic-hover-scroll-bottom': '长诊断提示底部',
    'entry-diagnostic-hover-scroll-reopen': '长诊断提示重开保留位置',
    'entry-diagnostic-select-partial': '诊断文字局部选择',
    'entry-diagnostic-select-whole': '诊断消息和代码全选',
    'entry-diagnostic-select-code': '诊断错误代码选择',
    'entry-diagnostic-select-wrapped': '诊断自动折行选择',
    'entry-diagnostic-select-overlap': '跨诊断消息选择',
    'entry-diagnostic-select-reverse': '反向跨诊断选择',
    'entry-diagnostic-select-word': '诊断双击选择单词',
    'entry-diagnostic-select-navigation': '问题导航正文选择',
    'entry-diagnostic-select-unicode': '诊断 Unicode 换行和 Tab 选择',
    'entry-diagnostic-hover': 'JSON 诊断悬浮提示',
    'entry-diagnostic-hover-dark': '深色 JSON 诊断提示',
    'entry-diagnostic-hover-overlap': '重叠 JSON 诊断提示',
    'entry-diagnostic-hover-overlap-dark': '深色重叠诊断提示',
    'entry-diagnostic-hover-full': '全屏 JSON 诊断提示',
    'entry-diagnostic-hover-short': '短消息诊断提示',
    'entry-diagnostic-hover-duplicate': '重复字段警告提示',
    'entry-diagnostic-hover-overlap-full': '全屏重叠诊断提示',
    'entry-diagnostic-navigation': 'JSON 问题导航',
    'entry-diagnostic-navigation-dark': '深色 JSON 问题导航',
    'entry-diagnostic-editor': 'JSON 编辑错误标记',
    'entry-diagnostic-editor-dark': '深色 JSON 编辑错误标记',
    'entry-diagnostic-warning': 'JSON 重复字段警告',
    'entry-diagnostic-unicode': 'JSON Emoji 错误范围',
    'entry-diagnostic-multiline': 'JSON CRLF 错误位置',
    'entry-diagnostic-wrapped': 'JSON 自动换行错误标记',
    'entry-conversion-initial': '首次行编辑转换等待',
    'entry-conversion-empty-pending': '空值草稿转换等待',
    'entry-conversion-empty-followup': '空值草稿后续等待',
    'entry-conversion-empty-ready': '空值转换保留可见草稿',
    'entry-conversion-pending': '格式转换等待',
    'entry-conversion-followup': '格式后续转换等待',
    'entry-preferences-saved': '保存设置保留全屏编辑',
    'entry-preferences': '编辑面板上的解码器设置',
    'entry-preferences-return': '取消设置保留编辑草稿',
    'entry-preferences-full': '全屏编辑上的解码器设置',
    'entry-preferences-dark': '深色编辑上的解码器设置',
    'entry-picker-format': '编辑格式菜单',
    'entry-picker-decode': '编辑解码菜单',
    'entry-picker-format-dark': '深色编辑格式菜单',
    'entry-picker-decode-dark': '深色编辑解码菜单',
    'entry-picker-format-hover': '编辑格式菜单悬停',
    'entry-picker-format-full': '全屏编辑格式菜单',
    'entry-picker-custom': '编辑自定义解码分组',
    'entry-picker-long': '编辑长解码名称',
    'entry-picker-long-menu': '编辑长名称解码菜单',
    'entry-picker-format-tooltip': '编辑格式悬停提示',
    'entry-picker-decode-tooltip': '编辑解码悬停提示',
    'entry-picker-format-tooltip-dark': '深色编辑格式提示',
    'edit-entry-hash': 'Hash 行编辑',
    'edit-entry-list': 'List 行编辑',
    'edit-entry-set': 'Set 行编辑',
    'edit-entry-zset': 'ZSet 行编辑',
    'edit-entry-hash-pinned': '固定编辑面板',
    'edit-entry-hash-draft': '编辑值草稿',
    'edit-entry-hash-full': '全屏编辑',
    'edit-entry-hash-full-dark': '深色全屏编辑',
    'edit-entry-hash-dark': '深色编辑草稿',
    'edit-entry-hash-field-focus': '字段输入焦点',
    'edit-entry-list-field-focus': '只读索引焦点',
    'edit-entry-hash-dark-default': '深色禁用更新',
    'edit-entry-hash-dark-field-focus': '深色字段焦点',
    'add-entries-hash': '添加 Hash 字段',
    'add-entries-list': '添加 List 元素',
    'add-entries-set': '添加 Set 成员',
    'add-entries-zset': '添加 ZSet 成员',
    'add-entries-stream': '添加 Stream 记录',
    'add-entries-hash-rows': '添加 Hash 动态行',
    'add-entries-hash-ignore': '忽略已有 Hash 字段',
    'add-entries-reopened': '添加元素重开记忆',
    'add-entries-list-prepend': 'List 前置插入',
    'add-entries-zset-rows': '添加 ZSet 值与分值',
    'add-entries-stream-rows': '添加 Stream 多字段',
    'add-entries-dark-hash': '深色添加 Hash 字段',
    'add-entries-dark-zset': '深色添加 ZSet 成员',
    'add-entries-binary-key': '二进制键添加字段',
    'add-entries-list-scroll': '添加元素长表单',
    'add-entries-saving': '添加元素等待请求',
    'new-key-string': '新建 String 键',
    'new-key-hash': '新建 Hash 键',
    'new-key-list': '新建 List 键',
    'new-key-set': '新建 Set 键',
    'new-key-zset': '新建 ZSet 键',
    'new-key-stream': '新建 Stream 键',
    'new-key-json': '新建 JSON 键',
    'new-key-required': '新建键必填提示',
    'new-key-type-menu': '新建键类型选择',
    'new-key-db-filter': '新建键数据库筛选',
    'new-key-db-empty': '新建键数据库无匹配',
    'new-key-hash-rows': '新建 Hash 动态行',
    'new-key-zset-rows': '新建 ZSet 成员与分值',
    'new-key-stream-value': '新建 Stream 字段',
    'new-key-dark-string': '深色新建 String',
    'new-key-dark-hash': '深色新建 Hash 动态行',
    'new-key-list-scroll': '新建键表单滚动',
    'new-key-value-resized': '新建键值文本框拉高',
    'new-key-value-minimum': '新建键值文本框最小高度',
    "csv-list-start": "CSV 长键列表首部",
    "csv-list-hover": "CSV 长列表滚动条悬停",
    "csv-list-drag": "CSV 长列表拖动滚动条",
    "csv-list-wheel": "CSV 长列表滚动位置",
    "csv-list-middle": "CSV 长键列表中部",
    "csv-list-selection": "CSV 长键列表选择",
    "csv-list-end": "CSV 长键列表底部",
    "csv-list-return": "CSV 长列表滚回与复制",
    "csv-list-dark-middle": "深色 CSV 长键列表",
    "csv-dark-import": "深色 CSV 导入",
    "csv-dark-custom": "深色 CSV 自定义 TTL",
    "csv-dark-unit": "深色 CSV TTL 单位菜单",
    "csv-dark-unit-hover": "深色 CSV TTL 单位悬停",
    "csv-dark-export-empty": "深色 CSV 无选中键",
    "csv-dark-export": "深色 CSV 导出",
    "csv-dark-export-ready": "深色 CSV 导出就绪",
    "csv-unit-menu": "CSV TTL 单位菜单",
    "csv-unit-hover": "CSV TTL 单位悬停",
    "csv-unit-keyboard": "CSV TTL 单位方向键",
    "csv-unit-selected": "CSV TTL 已选单位",
    "csv-unit-reopened": "CSV TTL 重开保留单位",
    "csv-unit-day": "CSV TTL 天数选中",
    "csv-export-select-plain": "CSV 受影响键文本",
    "csv-export-select-partial": "CSV 键名局部选择",
    "csv-export-select-lines": "CSV 跨键名选择",
    "csv-export-select-reverse": "CSV 反向跨行选择",
    "csv-export-select-word": "CSV 双击键名",
    "csv-export-select-binary": "CSV 二进制键选择",
    "csv-progress-initial": "CSV 初始加载消息",
    "csv-export-progress-loading": "CSV 导出进度与取消按钮",
    "csv-import-progress-loading": "CSV 导入进度与取消按钮",
    "csv-export-progress-success": "CSV 导出成功",
    "csv-export-progress-warning": "CSV 部分导出失败",
    "csv-export-progress-error": "CSV 导出失败计数",
    "csv-export-progress-cancel": "CSV 导出取消",
    "csv-export-progress-failure": "CSV 导出请求失败",
    "csv-import-progress-success": "CSV 导入完成",
    "csv-import-progress-no-reload": "CSV 导入保留列表",
    "csv-import-progress-cancel": "CSV 导入取消",
    "csv-import-progress-failure": "CSV 导入请求失败",
    "csv-import-default": "CSV 导入默认值",
    "csv-import-ignore": "CSV 忽略冲突与过期时间",
    "csv-import-custom": "CSV 自定义 TTL",
    "csv-import-custom-minutes": "CSV 分钟 TTL",
    "csv-import-reopened": "重新打开 CSV 导入",
    "csv-import-custom-reopened": "CSV 保留 TTL 单位",
    "csv-export-empty": "CSV 无选中键",
    "csv-export-default": "CSV 导出默认值",
    "csv-export-expire": "CSV 导出过期时间",
    "csv-export-reopened": "重新打开 CSV 导出",

    "refresh-detail": "键详情刷新设置",
    "refresh-detail-on": "启用键详情自动刷新",
    "refresh-detail-dark": "深色刷新设置",
    "refresh-detail-on-dark": "深色启用自动刷新",
    "key-input-focused": "键名只读输入焦点",
    "key-input-selection": "键名局部选择",
    "key-input-dark": "深色键名输入",
    "key-input-selection-dark": "深色键名局部选择",
    "scalar-string-plain": "String 文本",
    "scalar-string-dark": "深色 String 编辑",
    "scalar-string-memory-zero": "String 零内存隐藏",
    "scalar-string-json": "String JSON 格式",
    "scalar-string-draft": "String 草稿",
    "scalar-string-saved": "String 保存成功",
    "scalar-string-save-error": "String 保存失败",
    "scalar-string-saving": "String 保存中",
    "json-object": "JSON 对象编辑",
    "json-dark": "深色 JSON 编辑",
    "json-memory-zero": "JSON 零内存隐藏",
    "json-scalar": "JSON 标量",
    "json-draft": "JSON 草稿",
    "json-saved": "JSON 保存成功",
    "json-save-error": "JSON 保存失败",
    "json-saving": "JSON 保存中",
    "connections": "连接列表",
    "connections-selected": "选中连接与行内操作",
    "connections-groups": "连接分组",
    "connections-group-open": "展开分组与行内操作",
    "connections-filtered": "筛选连接",
    "connections-filter-group": "筛选分组",
    "connections-connected": "已连接状态",
    "connections-empty": "空连接列表",
    "connections-empty-group": "空分组",
    "connections-groups-dark": "深色分组",
    "connections-copy": "复制连接名称",
    "connections-keyboard": "连接树方向键",
    "connections-keyboard-filtered": "筛选后的方向键",
    "connections-menu": "连接右键菜单",
    "connections-menu-hover": "菜单悬停",
    "connections-menu-connected": "已连接右键菜单",
    "connections-menu-group": "分组右键菜单",
    "connections-menu-dark": "深色右键菜单",
    "connections-more": "连接备份菜单",
    "settings-menu": "设置菜单",
    "community": "公众号弹层",
    "message-success": "成功消息",
    "message-error": "错误消息",
    "message-error-modal": "弹窗上的错误消息",
    "message-error-dark": "深色错误消息",
    "message-stack": "多条消息",
    "notification-error": "参数错误通知",
    "notification-error-dark": "深色参数错误通知",
    "key-type-menu": "悬停类型菜单",
    "key-type-hash": "Hash 类型筛选",
    "key-type-menu-dark": "深色类型菜单",
    "key-database-menu": "数据库选择菜单",
    "key-database-filtered": "按编号筛选数据库",
    "key-database-empty": "数据库编号无匹配",
    "key-database-selected": "回车切换数据库",
    "key-database-keyboard": "数据库方向键选择",
    "key-database-alias-empty": "别名文字不匹配编号",
    "key-database-alias": "数据库别名与编号",
    "entries-hash": "Hash 表格",
    "entries-list": "List 表格",
    "entries-set": "Set 表格",
    "entries-zset": "ZSet 表格",
    "entries-stream": "Stream 表格",
    "entries-hash-dark": "深色 Hash 表格",
    "entries-hash-left": "表格左对齐",
    "entries-hash-field-filter": "按字段局部筛选",
    "entries-hash-value-filter": "Hash 值不匹配字段筛选",
    "entries-list-index-filter": "List 序号不匹配值筛选",
    "entries-list-value-filter": "List 值筛选",
    "entries-hash-sort": "Hash 字段排序",
    "entries-zset-sort": "ZSet 分数排序",
    "entries-hash-resize": "拖动 Hash 列宽",
    "entries-hash-delete": "Hash 行删除确认",
    "entries-hash-delete-dark": "深色行删除确认",
    "footer-hash": "集合底部工具栏",
    "footer-format-menu": "格式选择菜单",
    "footer-decode-menu": "解码选择菜单",
    "footer-hash-dark": "深色底部工具栏",
    "footer-format-menu-dark": "深色格式菜单",
    "footer-decode-menu-dark": "深色解码菜单",
    "footer-memory-zero": "隐藏零内存占用",
    "footer-stream": "Stream 底部工具栏",
    "footer-format-tooltip": "格式选择悬停提示",
    "footer-decode-tooltip": "解码选择悬停提示",
    "footer-format-tooltip-dark": "深色格式悬停提示",
    "footer-decode-custom": "自定义解码器分组",
    "footer-decode-long": "长解码器名称",
    "footer-decode-long-menu": "长名称解码菜单",
    "footer-hash20": "20px 通用字号底栏 macOS 原版",
    "footer-decode-long20": "20px 通用字号长解码器 macOS 原版",
    "footer-decode-long-menu20": "20px 通用字号解码菜单 macOS 原版",
    "content-nav-status": "导航状态页",
    "content-nav-key_detail": "导航键详情",
    "content-nav-cli": "导航命令行",
    "content-nav-slow_log": "导航慢日志",
    "content-nav-cmd_monitor": "导航监控命令",
    "content-nav-pub_message": "导航发布订阅",
    "content-nav-dark": "深色导航与键工具栏",
    "content-nav-large": "大字号下的导航控件",
    "key-toolbar-binary": "二进制键工具栏",
    "key-toolbar-ttl": "TTL 倒计时工具栏",
    "key-toolbar-ttl-days": "TTL 天数显示",
    "key-toolbar-ttl-zero": "TTL 零值显示",
    "content-nav-narrow": "窄窗口导航",
    "content-nav-narrow-pubsub": "窄窗口末尾标签",
    "key-browser": "键浏览器",
    "key-browser-folder-selected": "选中键分组",
    "key-browser-folder-open": "展开键分组",
    "key-browser-key-selected": "选中键",
    "key-browser-filtered": "局部筛选",
    "key-browser-check-mode": "级联勾选模式",
    "key-browser-search": "Redis 搜索标签",
    "key-browser-menu-folder": "键分组菜单",
    "key-browser-menu-key": "键右键菜单",
    "key-browser-more": "数据库操作菜单",
    "key-browser-delete": "删除单键确认",
    "welcome": "首次欢迎提示", "sponsor": "赞助展示", "new-connection": "新建连接",
    "connection-advanced": "高级配置", "connection-alias": "数据库别名",
    "connection-ssl": "SSL / TLS", "connection-ssh": "SSH 隧道",
    "connection-sentinel": "哨兵模式", "connection-cluster": "集群模式",
    "connection-proxy": "网络代理",
    "connection-ssl-enabled": "启用 SSL / TLS",
    "connection-ssh-enabled": "SSH 密码登录",
    "connection-ssh-pkfile": "SSH 私钥登录",
    "connection-ssh-agent": "SSH agent 登录",
    "connection-sentinel-enabled": "启用哨兵模式",
    "connection-proxy-custom": "手动配置代理",
    "connection-proxy-auth": "代理身份验证",
    "connection-alias-row": "数据库别名条目",
    "connection-advanced-tags": "数据库筛选标签",
    "connection-test-success": "连接测试成功",
    "connection-test-failure": "连接测试失败",
    "connection-validation-general": "常规字段错误",
    "connection-validation-name": "连接名非法字符",
    "connection-validation-advanced": "高级字段错误",
    "connection-validation-fixed": "纠正连接名",
    "connection-edit-incomplete": "编辑未完整连接",
    "connection-edit-incomplete-ssh": "编辑未完整 SSH",
    "preferences": "常规偏好",
    "font-size-preview": "常规字号实时预览",
    "font-size-cleared": "清空字号后的预览",
    "preferences-font-filter": "字体筛选与手动输入",
    "preferences-font-keyboard": "方向键选择字体",
    "preferences-font-tags": "字体多选标签",
    "preferences-editor": "编辑器偏好", "preferences-cli": "命令行偏好",
    "preferences-decoder": "自定义解码", "status": "服务器状态",
    "cli": "命令行", "cli-results": "命令行回复",
    "cli-ansi": "终端 ANSI 样式", "cli-long": "终端长行换行", "cli-selection": "终端跨行选择",
    "cli-font-size": "命令行字号与行列",
    "cli-word-selection": "命令行双击选词",
    "string": "字符串", "hash": "Hash", "hash-edit": "Hash 行编辑",
    "list": "List", "set": "Set", "zset": "ZSet", "stream": "Stream", "ttl": "TTL",
}


def image(path):
    return "data:image/png;base64," + base64.b64encode(path.read_bytes()).decode()


scenes = []
missing = []
for name, label in NAMES.items():
    native = ROOT / "outputs/ui-qa" / (name + ".png")
    reference = ROOT / "outputs/ui-reference" / (name + ".png")
    if native.exists() and reference.exists():
        scenes.append({"name": label, "native": image(native), "reference": image(reference)})
    else:
        missing.append(name)
if missing:
    raise SystemExit("Missing comparison images: " + ", ".join(missing))

# Keep historical comparisons intact and append the independent Mac captures.
# Set scan order and generated Stream IDs differ between the fixed source props
# and the owned Redis fixture; these pairs expose those differences as captured.
for fixture_name in ("entries-macos-source.json", "entries-overlays-macos-source.json", "key-filter-macos-source.json", "tag-style-macos-source.json", "tag-overflow-macos-source.json", "sidebar-resize-macos-source.json", "sidebar-contract-macos-source.json", "selector-font-macos-source.json", "general-fallback-macos-source.json", "css-label-macos-source.json"):
    fixture = json.loads((ROOT / "tests/fixtures" / fixture_name).read_text())
    for record in fixture["records"]:
        name = record["name"]
        if fixture_name == "entries-overlays-macos-source.json" and not name.startswith("entries-"):
            continue
        native = ROOT / "outputs/ui-qa" / (name + ".png")
        reference = ROOT / fixture["archiveDirectory"] / (name + ".png")
        if fixture_name == "key-filter-macos-source.json":
            native = ROOT / "outputs/ui-qa-key-filter-crops" / (name + ".png")
            reference = ROOT / "outputs/ui-reference-key-filter-crops" / (name + ".png")
        if fixture_name == "tag-style-macos-source.json":
            native = ROOT / "outputs/ui-qa-tag-style-crops" / (name + ".png")
            reference = ROOT / "outputs/ui-reference-tag-style-crops" / (name + ".png")
        if fixture_name == "tag-overflow-macos-source.json":
            native = ROOT / "outputs/ui-qa-tag-overflow-crops" / (name + ".png")
            reference = ROOT / "outputs/ui-reference-tag-overflow-crops" / (name + ".png")
        if fixture_name.startswith("sidebar-"):
            native = ROOT / "outputs/ui-qa-sidebar-crops" / (name + ".png")
            reference = ROOT / "outputs/ui-reference-sidebar-crops" / (name + ".png")
        if fixture_name in ("selector-font-macos-source.json", "general-fallback-macos-source.json"):
            native = ROOT / "outputs/ui-qa-selector-font-crops" / (name + ".png")
            reference = ROOT / "outputs/ui-reference-selector-font-crops" / (name + ".png")
        if fixture_name == "css-label-macos-source.json":
            native = ROOT / "outputs/ui-qa-css-label-crops" / (name + ".png")
            reference = ROOT / "outputs/ui-reference-css-label-crops" / (name + ".png")
        if not native.exists() or not reference.exists():
            raise SystemExit("Missing Mac comparison images: " + name)
        scenes.append({"name": "macOS · " + NAMES.get(name, name),
                       "native": image(native), "reference": image(reference)})
if not scenes:
    raise SystemExit("Render the native and reference QA images first")

HTML = """<!doctype html><html lang="zh-CN"><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>Rust RDM 桌面界面对照</title><style>
*{box-sizing:border-box}body{margin:0;background:#f5f5f6;color:#333639;font:14px system-ui,-apple-system,sans-serif}
header{padding:24px max(24px,calc((100vw - 1100px)/2));background:white;border-bottom:1px solid #ddd}
h1{margin:0 0 10px;font-size:22px}p{margin:8px 0;line-height:1.7;color:#62666c}
.controls{display:flex;flex-wrap:wrap;gap:16px;align-items:center;margin-top:16px}
select,button{font:inherit;height:34px;border:1px solid #ddd;border-radius:4px;background:white;padding:0 12px}
label{display:flex;align-items:center;gap:8px}main{max-width:1120px;margin:24px auto;padding:0 20px}
.labels{display:flex;justify-content:space-between;margin-bottom:8px;font-weight:600}
.scene{position:relative;aspect-ratio:4/3;border:1px solid #dedee5;border-radius:8px;overflow:hidden;background:white}
.scene img{position:absolute;inset:0;width:100%;height:100%;object-fit:contain}
#native{clip-path:inset(0 50% 0 0)}#line{position:absolute;left:50%;top:0;bottom:0;border-left:2px solid #d33a31;pointer-events:none}
#line:after{content:'↔';position:absolute;top:50%;left:-15px;border-radius:50%;background:#d33a31;color:white;width:30px;height:30px;display:grid;place-items:center}
footer{margin-top:18px;color:#666;font-size:13px;line-height:1.8}input[type=range]{accent-color:#d33a31;width:230px}
</style><header><h1>Rust RDM 桌面界面对照</h1>
<p>原版参考图来自 Vue 源码的临时副本和固定布局样本；GPUI 图使用临时 Redis。状态数值和行顺序可因样本不同而变化。</p>
<p>20px 底栏三项使用原版 macOS WKWebView 和九种内置解码器能力的固定样本；通用字号变化时，原版按钮与菜单仍使用 14px。其余截图保留各自的历史参考引擎。</p>
<p>左侧筛选栏包含 22 对输入／操作样本、6 对匹配标签／禁用搜索和 6 对长标签裁切样本的原始像素裁片，仅用于查看该控件；两套样本右侧模型不同，完整原图保留在验收文件中。</p>
<p>字体与类型另有 20 对原始头部裁片，包含七种类型、全部类型和缺失字体回退。另有 8 对 Helvetica、Courier 与中文/Emoji 的基线样本；历史失败仍保留，本页不代表整图或原生窗口验收通过。</p>
<p>另附 16 对侧栏头部裁片，覆盖明暗主题下的拖动、悬停、按下和超过窗口的宽度；只包含截图内实际可见像素，窗口外区域及动画中间帧未验收。</p>
<p>另附 24 对 macOS 集合表格和两对删除确认框；历史参考保留。Set 按 Redis 扫描顺序显示，Stream ID 来自不同固定样本，图中保留实际数据差异。</p>
<p>完整一致性验收仍在进行。仍有部分表格、字体控件样式和 CLI 待对齐；更多原生交互、其他弹窗待补齐。</p>
<p>JSON Emoji 样本的标记横向范围仍相差 3 个物理像素；诊断悬浮层与问题导航已有十幅限定对照；更多输入、滚动、字体、原生窗口与整图一致性仍待验收。</p>
<div class="controls"><label>页面<select id="pages" aria-label="选择对照页面"></select></label>
<label>分界<input id="split" type="range" min="0" max="100" value="50" aria-label="GPUI 与原版图片分界"></label>
<button id="nativeOnly">查看 GPUI</button><button id="referenceOnly">查看原版</button><button id="both">各显示一半</button></div>
</header><main><div class="labels"><span>GPUI</span><span>原版参考</span></div>
<div class="scene"><img id="reference" alt="原版参考布局"><img id="native" alt="Rust GPUI 布局"><div id="line"></div></div>
<footer>此页包含 SCENE_COUNT 组原版与 GPUI 截图，用于查看布局差异。完整验收范围、测试结果和待办见仓库 docs/parity.md。</footer></main>
<script>const scenes=SCENES;const pages=document.getElementById('pages'),split=document.getElementById('split'),native=document.getElementById('native'),reference=document.getElementById('reference'),line=document.getElementById('line');
scenes.forEach((scene,index)=>{const option=document.createElement('option');option.value=index;option.textContent=scene.name;pages.appendChild(option)});
function page(){const scene=scenes[Number(pages.value)];native.src=scene.native;reference.src=scene.reference}
function divide(value){split.value=value;native.style.clipPath='inset(0 '+(100-value)+'% 0 0)';line.style.left=value+'%';line.style.display=value===0||value===100?'none':'block'}
pages.onchange=page;split.oninput=()=>divide(Number(split.value));document.getElementById('nativeOnly').onclick=()=>divide(100);document.getElementById('referenceOnly').onclick=()=>divide(0);document.getElementById('both').onclick=()=>divide(50);page();
</script></html>"""
output = ROOT / "outputs/parity-review.html"
output.write_text(HTML.replace("SCENES", json.dumps(scenes, ensure_ascii=False)).replace("SCENE_COUNT", str(len(scenes))), encoding="utf-8")
print(f"Prepared {len(scenes)} page comparisons: {output}")
