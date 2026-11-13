import type { T } from "./i18n";

/**
 * 把 rclone 日志里的报错翻成「人话 + 该怎么办」。
 *
 * rclone 的原话对普通用户基本没法用：`CRITICAL: Fatal error received - 401 Unauthorized`
 * 既不说哪儿错了，也不说怎么改。这里按最常见的几类做模式匹配，
 * 命中就给一句建议，命不中就返回空串（界面只显示原始日志）。
 *
 * 规则按「越具体越靠前」排列：证书错误里也常带 connection 字样，
 * 401 的响应里也可能出现 html，所以顺序不能随便调。
 */
const RULES: { re: RegExp; key: string }[] = [
  // 位置被占：用户能自己解决，指向冲突弹窗
  { re: /mountpoint is not empty|mount point is not empty|directory not empty/i, key: "err.busy" },
  // 驱动缺失
  { re: /winfsp|fuse: device not found|fusermount|dokan|fuse isn't installed|no fuse/i, key: "err.driver" },
  // SMB 认证失败：比通用 401 更具体，得放在 401 前面才能命中
  // （0xC000006A 密码错 / 0xC0000064 用户不存在 / 0xC0000234 账户被锁）
  {
    re: /attempted logon is invalid|logon failure|bad username|nt[ _-]?status.*logon|0xc000006[4a]|0xc0000234|account (is )?(currently )?locked/i,
    key: "err.smbAuth",
  },
  // 认证
  { re: /\b401\b|unauthorized|authentication failed|auth failed|invalid credentials|登录失败/i, key: "err.401" },
  { re: /\b403\b|forbidden/i, key: "err.403" },
  // 共享 / 路径不存在（SMB 共享名写错最常见；BAD_NETWORK_NAME 是 SMB 树连接失败的原话）
  {
    re: /share not found|path not found|directory not found|tree connect|bad network name|\b404\b|no such file or directory/i,
    key: "err.404",
  },
  // 证书
  { re: /x509|certificate|tls handshake|unknown authority|self[- ]signed/i, key: "err.cert" },
  // DNS
  { re: /no such host|name resolution|dns|server misbehaving|lookup .* failed/i, key: "err.dns" },
  // 连接被拒 / 端口不对
  { re: /connection refused|connect: refused|actively refused/i, key: "err.refused" },
  // 超时 / 网络不可达 / 连接中途断开
  {
    re: /timeout|timed out|i\/o timeout|connection reset|network is unreachable|no route to host|server closed|unexpected eof|broken pipe/i,
    key: "err.timeout",
  },
  // 权限
  { re: /access is denied|permission denied|operation not permitted|拒绝访问/i, key: "err.denied" },
  // 容量满 / 配额
  { re: /no space left|quota exceeded|disk quota|insufficient (disk )?space/i, key: "err.quota" },
  // 密码里混了全角字符：这个坑踩过很多次，单独提示
  { re: /非法|invalid password|密码/i, key: "err.password" },
];

/** 返回一句本地化的建议；认不出来的错误返回空串 */
export function explainError(raw: string, t: T): string {
  if (!raw) return "";
  for (const r of RULES) {
    if (r.re.test(raw)) return t(r.key);
  }
  return "";
}
