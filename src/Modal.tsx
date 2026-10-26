import { useRef, type ReactNode } from "react";

/**
 * 统一的弹窗外壳：所有弹窗（新建连接、设置）共用同一套结构与样式。
 *
 * 关闭行为经过修正——只有「按下和松开都发生在遮罩空白处」才关闭。
 * 之前直接在遮罩上监听 click，在输入框里拖拽选中文本、鼠标移出窗口后
 * 松开，WebView 会把 click 派发到遮罩上，导致弹窗意外关闭。
 */
export default function Modal({
  title,
  subtitle,
  onClose,
  children,
  footer,
  size = "",
  closeLabel = "Close",
}: {
  title: string;
  subtitle?: string;
  onClose: () => void;
  children: ReactNode;
  footer?: ReactNode;
  size?: string;
  closeLabel?: string;
}) {
  const downOnBackdrop = useRef(false);

  return (
    <div
      className="modal-backdrop"
      onMouseDown={(e) => {
        downOnBackdrop.current = e.target === e.currentTarget;
      }}
      onClick={(e) => {
        if (downOnBackdrop.current && e.target === e.currentTarget) onClose();
      }}
    >
      <div className={`modal ${size}`} role="dialog" aria-modal="true">
        <header className="modal-head">
          <div>
            <h2>{title}</h2>
            {subtitle && <p className="muted small">{subtitle}</p>}
          </div>
          <button className="icon-btn" onClick={onClose} title={closeLabel} aria-label={closeLabel}>
            ✕
          </button>
        </header>
        <div className="modal-body">{children}</div>
        {footer && <footer className="modal-foot">{footer}</footer>}
      </div>
    </div>
  );
}
