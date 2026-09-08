// 快捷键冲突的单一事实源（纯数据+纯函数，无 IPC）。
// 分工：硬冲突（四键互斥、合法性）以后端 settings_service 为权威——保存时被拒绝；
// 软冲突（编辑器内置绑定/系统常用键）本模块是唯一事实源——录制时即时警告、不阻止保存。
// 表内键位全部实读自 node_modules/@tiptap/extension-*/src（Mod 在 Windows 即 Ctrl）。

/** 把设置里存的原始组合（如 'ctrl+B'、'Esc'）归一化成比较键（'CTRL+B'、'ESCAPE'）。
 *  自 App.tsx 顶层迁入（行为不变），App 与录制框共用同一实现，避免归一化漂移。 */
export function normalizeShortcutSetting(val?: string): string {
  if (!val) return ''
  const parts = val
    .split('+')
    .map((s) => s.trim().toUpperCase())
    .filter(Boolean)
  const hasCtrl = parts.some((p) => p === 'CTRL' || p === 'CONTROL')
  const hasAlt = parts.some((p) => p === 'ALT')
  const hasShift = parts.some((p) => p === 'SHIFT')
  const hasSuper = parts.some((p) => p === 'SUPER' || p === 'WIN' || p === 'META')

  const keyPart = parts.find(
    (p) => !['CTRL', 'CONTROL', 'ALT', 'SHIFT', 'SUPER', 'WIN', 'META'].includes(p)
  )

  let key = keyPart || ''
  if (key === 'ESC') key = 'ESCAPE'
  if (key === ' ' || key === 'SPACEBAR') key = 'SPACE'

  const result: string[] = []
  if (hasCtrl) result.push('CTRL')
  if (hasAlt) result.push('ALT')
  if (hasShift) result.push('SHIFT')
  if (hasSuper) result.push('SUPER')
  if (key) result.push(key)
  return result.join('+')
}

// tiptap 编辑器内置键位 → 动作名。证据（@tiptap 2.27.2 实读）：
// bold.ts:109 / italic.ts:111 / code.ts:97 / history.ts:75-77 / strike.ts:101 /
// blockquote.ts:82 / bullet-list.ts:99 / ordered-list.ts:123 / task-list.ts:74 /
// code-block.ts:158（切换）、:178（三连 Enter 退出）/ paragraph.ts:63 / heading.ts:112 /
// hard-break.ts:110-111 / @tiptap/core dist baseKeymap（CTRL+A/CTRL+BACKSPACE 等）。
export const EDITOR_ACTION_LABELS: Readonly<Record<string, string>> = {
  'CTRL+B': '加粗',
  'CTRL+I': '斜体',
  'CTRL+E': '行内代码',
  'CTRL+Z': '撤销',
  'CTRL+SHIFT+Z': '重做',
  'CTRL+Y': '重做',
  'CTRL+SHIFT+S': '删除线',
  'CTRL+SHIFT+B': '引用块',
  'CTRL+SHIFT+8': '无序列表',
  'CTRL+SHIFT+7': '有序列表',
  'CTRL+SHIFT+9': '任务列表',
  'CTRL+ALT+C': '代码块',
  'CTRL+ALT+0': '正文段落',
  'CTRL+ALT+1': '一级标题',
  'CTRL+ALT+2': '二级标题',
  'CTRL+ALT+3': '三级标题',
  'CTRL+A': '全选',
  'CTRL+ENTER': '硬换行',
  'SHIFT+ENTER': '硬换行',
  'CTRL+BACKSPACE': '删除前词',
  'SHIFT+BACKSPACE': '删除',
  'CTRL+DELETE': '删除后词',
  ENTER: '分段/列表拆分',
  TAB: '缩进/列表升级',
  'SHIFT+TAB': '反缩进/列表降级',
  BACKSPACE: '退格',
  DELETE: '删除'
}

// 正文导航/编辑硬键：与 Editor.tsx 的 EDITOR_CRITICAL_KEYS 同源——这类键即使被配置成
// 应用快捷键也在正文内判给编辑器（不放行），所以警告文案与修饰组合类不同
const EDITOR_SILENT_KEYS = new Set([
  'ENTER', 'TAB', 'SHIFT+TAB', 'BACKSPACE', 'DELETE'
])

// 系统/浏览器常用键（软警告）。WebView2 的浏览器加速键与宿主环境相关，措辞用“可能被优先”
export const SYSTEM_KEY_LABELS: Readonly<Record<string, string>> = {
  'CTRL+F': '浏览器查找',
  'CTRL+G': '浏览器查找下一处',
  'CTRL+P': '打印',
  'CTRL+S': '网页保存',
  'CTRL+R': '刷新',
  'CTRL+O': '打开文件',
  'CTRL+L': '定位地址栏/输入区',
  'CTRL+K': '浏览器搜索',
  'CTRL+D': '收藏',
  F3: '查找重复',
  F5: '刷新',
  F12: '开发者工具',
  'CTRL+=': '缩放放大',
  'CTRL+-': '缩放缩小',
  'CTRL+0': '缩放重置'
}

export type ConflictKind = 'app' | 'editor' | 'system'

export interface ConflictResult {
  kind: ConflictKind
  message: string
}

/**
 * 检测一个（已归一化的）组合与其他配置的冲突。
 * @param comboNorm 归一化后的候选组合（normalizeShortcutSetting 的输出形态）
 * @param others    其余三项（同表归一化形态；空串调用方已过滤）
 * @param includeEditor 是否检查编辑器内置绑定——全局召唤键判 false：
 *                      它是系统级 RegisterHotKey，命中时按键根本不进页面，
 *                      “正文内让位”文案对它无意义；仅检查互斥与系统键占用。
 */
export function detectConflict(
  comboNorm: string,
  others: Array<{ label: string; combo: string }>,
  includeEditor: boolean
): ConflictResult | null {
  if (!comboNorm) return null

  for (const o of others) {
    if (o.combo === comboNorm) {
      return {
        kind: 'app',
        message: `与“${o.label}”重复，保存将被拒绝——请换一个组合`
      }
    }
  }

  if (includeEditor) {
    const ed = EDITOR_ACTION_LABELS[comboNorm]
    if (ed) {
      if (EDITOR_SILENT_KEYS.has(comboNorm)) {
        return {
          kind: 'editor',
          message: `“${comboNorm}”是正文编辑键：在便签正文中不会触发该动作（编辑器保留），建议用带修饰键的组合`
        }
      }
      return {
        kind: 'editor',
        message: `编辑器内绑定“${ed}”：保存后在便签正文中此键执行应用快捷键，“${ed}”让位`
      }
    }
  }

  const sys = SYSTEM_KEY_LABELS[comboNorm]
  if (sys) {
    return {
      kind: 'system',
      message: `是系统/浏览器常用键（${sys}），部分场景可能被优先占用，建议避开`
    }
  }

  return null
}
