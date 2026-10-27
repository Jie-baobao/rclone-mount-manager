import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import SendToWindow from "./SendToWindow";
import "./styles.css";

/**
 * 同一个前端包被两种窗口复用：
 *   - 主窗口（无标记）        → 完整管理器
 *   - 「发送到」小窗          → 只有确认框，不渲染侧栏 / 连接列表
 *
 * 小窗由后端用 initialization_script 打上 __RMM_SENDTO__ 标记（见 sendto.rs）；
 * ?sendto=1 只是方便浏览器里直接预览这个界面。
 */
const isSendTo =
  (window as { __RMM_SENDTO__?: number }).__RMM_SENDTO__ === 1 ||
  new URLSearchParams(window.location.search).get("sendto") === "1";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>{isSendTo ? <SendToWindow /> : <App />}</React.StrictMode>
);
