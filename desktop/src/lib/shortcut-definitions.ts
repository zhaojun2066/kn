import { altKey, modKey } from "../utils/shortcut";

export interface ShortcutItem {
  keys: string[];
  desc: string;
  note?: string;
}

export interface ShortcutGroup {
  id: string;
  title: string;
  description: string;
  items: ShortcutItem[];
}

export function getShortcutGroups(): ShortcutGroup[] {
  const mod = modKey();
  const alt = altKey();

  return [
    {
      id: "global",
      title: "全局",
      description: "在主窗口大多数位置可用",
      items: [
        { keys: [mod, "P"], desc: "快速切换运行配置" },
        { keys: [mod, "⇧", "P"], desc: "打开会话历史" },
        { keys: [mod, "N"], desc: "新建运行配置" },
        { keys: [mod, "⇧", "G"], desc: "打开运行配置管理" },
        { keys: [mod, "⇧", "Y"], desc: "打开扩展管理" },
        { keys: [mod, "B"], desc: "显示 / 隐藏侧边栏" },
        { keys: [mod, "K"], desc: "打开 / 关闭快捷键帮助" },
      ],
    },
    {
      id: "navigation",
      title: "导航与弹窗",
      description: "用于列表、搜索、弹窗和快速切换",
      items: [
        { keys: ["↑", "↓"], desc: "在项目、运行配置、快速切换列表中移动选择" },
        { keys: ["Enter"], desc: "运行当前选择 / 确认当前选择" },
        { keys: ["Esc"], desc: "关闭弹窗、快速切换或取消当前选择" },
        { keys: [mod, "F"], desc: "聚焦当前列表搜索框", note: "终端聚焦时会搜索终端输出" },
        { keys: ["Tab"], desc: "在弹窗控件间前进" },
        { keys: ["⇧", "Tab"], desc: "在弹窗控件间后退" },
      ],
    },
    {
      id: "selection",
      title: "列表选择",
      description: "运行配置、扩展资源和 Hooks 列表支持",
      items: [
        { keys: [mod, "Click"], desc: "添加 / 移除单项选择" },
        { keys: ["⇧", "Click"], desc: "范围选择" },
        { keys: ["Checkbox", "Click"], desc: "勾选项目但不打开详情" },
        { keys: ["Backspace"], desc: "删除选中的运行配置", note: "输入框聚焦时不会触发" },
        { keys: ["Right Click"], desc: "打开上下文菜单" },
      ],
    },
    {
      id: "terminal",
      title: "终端",
      description: "底部终端和右侧终端面板",
      items: [
        { keys: ["Ctrl", "`"], desc: "显示 / 隐藏底部终端" },
        { keys: [mod, "J"], desc: "显示 / 隐藏底部终端" },
        { keys: [mod, "F"], desc: "搜索当前终端输出" },
        { keys: ["Esc"], desc: "关闭终端搜索" },
        { keys: [mod, "⇧", "M"], desc: "最大化 / 还原当前终端面板" },
        { keys: ["↑", "↓"], desc: "浏览终端历史命令" },
        { keys: ["Ctrl", "L"], desc: "清屏" },
        { keys: ["Ctrl", "C"], desc: "终止当前进程" },
      ],
    },
    {
      id: "panes",
      title: "终端窗格",
      description: "先点击终端面板后使用",
      items: [
        { keys: [mod, "D"], desc: "左右分屏" },
        { keys: [mod, "\\"], desc: "上下分屏" },
        { keys: [mod, "⇧", "D"], desc: "上下分屏（备选）" },
        { keys: [mod, "W"], desc: "关闭当前窗格" },
        { keys: [mod, alt, "←/↑/↓/→"], desc: "按方向切换窗格" },
        { keys: [mod, "]"], desc: "切换到下一个窗格" },
        { keys: [mod, "["], desc: "切换到上一个窗格" },
        { keys: [mod, "⇧", "Enter"], desc: "缩放 / 还原当前窗格" },
        { keys: [alt, "Click"], desc: "点击运行按钮时在新窗格运行" },
      ],
    },
  ];
}
