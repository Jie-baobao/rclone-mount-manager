/**
 * 复制文本到剪贴板。
 *
 * 先走标准 Clipboard API；在少数环境里（未聚焦的窗口、老版本 WebView）
 * 它会直接拒绝，这时退回「隐藏 textarea + execCommand」这条老路。
 */
export async function copyText(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    try {
      const ta = document.createElement("textarea");
      ta.value = text;
      ta.style.position = "fixed";
      ta.style.top = "-1000px";
      ta.style.opacity = "0";
      document.body.appendChild(ta);
      ta.focus();
      ta.select();
      const ok = document.execCommand("copy");
      document.body.removeChild(ta);
      return ok;
    } catch {
      return false;
    }
  }
}
