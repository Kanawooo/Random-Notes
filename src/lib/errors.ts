/**
 * 统一错误消息提取。Tauri v2 invoke 的 reject 值就是后端返回的 string
 * （core.js 原样 reject，无 InvokeError 包装），Error 与其余类型一并覆盖。
 */
export function toErrMsg(err: unknown): string {
  if (err instanceof Error) return err.message
  if (typeof err === 'string') return err
  return String(err)
}
