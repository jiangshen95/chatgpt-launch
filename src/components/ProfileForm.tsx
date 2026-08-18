import { useState } from "react";
import type { Profile, ProxyProtocol } from "../types";
import { LANGUAGES, TIMEZONES } from "../constants";

interface Props {
  initial: Profile | null;
  onSave: (p: Profile) => void;
  onCancel: () => void;
}

export function ProfileForm({ initial, onSave, onCancel }: Props) {
  const [name, setName] = useState(initial?.name ?? "");
  const [protocol, setProtocol] = useState<ProxyProtocol>(initial?.proxy.protocol ?? "socks5");
  const [host, setHost] = useState(initial?.proxy.host ?? "127.0.0.1");
  const [port, setPort] = useState<number>(initial?.proxy.port ?? 1080);
  const [username, setUsername] = useState(initial?.proxy.username ?? "");
  const [password, setPassword] = useState(initial?.proxy.password ?? "");
  const [timezone, setTimezone] = useState(initial?.timezone ?? "");
  const [language, setLanguage] = useState(initial?.language ?? "");
  const [injectTimezone, setInjectTimezone] = useState(initial?.injection?.injectTimezone ?? false);
  const [injectLanguage, setInjectLanguage] = useState(initial?.injection?.injectLanguage ?? false);
  const [leakProtection, setLeakProtection] = useState(initial?.injection?.leakProtection ?? true);
  const [syncCodexEnv, setSyncCodexEnv] = useState(initial?.injection?.syncCodexEnv ?? true);
  const [appPath, setAppPath] = useState(initial?.appPath ?? "");

  const submit = () => {
    if (!name.trim()) return alert("请填写名称");
    if (!host.trim()) return alert("请填写代理主机");
    if (!Number.isInteger(port) || port < 1 || port > 65535) return alert("端口无效");

    const profile: Profile = {
      id: initial?.id ?? "",
      name: name.trim(),
      proxy: {
        protocol,
        host: host.trim(),
        port,
        username: username.trim() || null,
        password: password || null,
      },
      timezone: timezone || null,
      language: language || null,
      telemetry: initial?.telemetry ?? {
        disableSentry: false,
        disableStatsig: false,
        disableSparkle: false,
      },
      injection: {
        injectTimezone,
        injectLanguage,
        leakProtection,
        syncCodexEnv,
      },
      appPath: appPath.trim() || null,
      createdAt: initial?.createdAt ?? 0,
      updatedAt: initial?.updatedAt ?? 0,
    };
    onSave(profile);
  };

  return (
    <div className="overlay">
      <div className="modal">
        <h2>{initial?.id ? "编辑配置" : "新建配置"}</h2>

        <label className="field">
          <span>名称</span>
          <input
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="例如：美国住宅-洛杉矶"
            autoFocus
          />
        </label>

        <div className="grid-3">
          <label className="field">
            <span>协议</span>
            <select
              value={protocol}
              onChange={(e) => setProtocol(e.target.value as ProxyProtocol)}
            >
              <option value="socks5">SOCKS5</option>
              <option value="http">HTTP</option>
              <option value="https">HTTPS</option>
            </select>
          </label>
          <label className="field">
            <span>主机</span>
            <input value={host} onChange={(e) => setHost(e.target.value)} placeholder="127.0.0.1" />
          </label>
          <label className="field">
            <span>端口</span>
            <input
              type="number"
              value={port}
              min={1}
              max={65535}
              onChange={(e) => setPort(Number(e.target.value))}
            />
          </label>
        </div>

        <div className="grid-2">
          <label className="field">
            <span>用户名（可选）</span>
            <input value={username} onChange={(e) => setUsername(e.target.value)} />
          </label>
          <label className="field">
            <span>密码（可选）</span>
            <input type="password" value={password} onChange={(e) => setPassword(e.target.value)} />
          </label>
        </div>

        <div className="grid-2">
          <label className="field">
            <span>时区</span>
            <select value={timezone} onChange={(e) => setTimezone(e.target.value)}>
              <option value="">自动（根据代理出口）</option>
              {TIMEZONES.map((t) => (
                <option key={t} value={t}>
                  {t}
                </option>
              ))}
            </select>
          </label>
          <label className="field">
            <span>语言</span>
            <select value={language} onChange={(e) => setLanguage(e.target.value)}>
              <option value="">自动（根据代理出口）</option>
              {LANGUAGES.map((l) => (
                <option key={l} value={l}>
                  {l}
                </option>
              ))}
            </select>
          </label>
        </div>

        <label className="field">
          <span>应用路径（可选，留空自动检测）</span>
          <input
            value={appPath}
            onChange={(e) => setAppPath(e.target.value)}
            placeholder="/Applications/ChatGPT.app/Contents/MacOS/ChatGPT"
          />
        </label>

        <div className="telemetry">
          <div className="telemetry-title">注入与防护选项</div>
          <label className="check">
            <input
              type="checkbox"
              checked={injectTimezone}
              onChange={(e) => setInjectTimezone(e.target.checked)}
            />{" "}
            注入时区（默认关）
          </label>
          <label className="check">
            <input
              type="checkbox"
              checked={injectLanguage}
              onChange={(e) => setInjectLanguage(e.target.checked)}
            />{" "}
            注入语言（默认关）
          </label>
          <label className="check">
            <input
              type="checkbox"
              checked={leakProtection}
              onChange={(e) => setLeakProtection(e.target.checked)}
            />{" "}
            WebRTC / QUIC / DNS 泄露防护（默认开）
          </label>
          <label className="check">
            <input
              type="checkbox"
              checked={syncCodexEnv}
              onChange={(e) => setSyncCodexEnv(e.target.checked)}
            />{" "}
            同步代理到 ~/.codex/.env（默认开）
          </label>
          {(injectTimezone || injectLanguage) && (
            <p className="warn-text">
              ⚠ 注入时区/语言只改浏览器渲染层，不会改操作系统本身，也不影响桌面版遥测
              （Sentry/Statsig）上报的真实系统信息。若系统时区/语言与出口不一致，注入反而
              可能造成「浏览器层=出口、系统层=本地」的跨层矛盾，更易被识别。建议先手动把
              系统时区/区域/语言改为与出口一致，再开启。
            </p>
          )}
        </div>

        <div className="telemetry">
          <div className="telemetry-title">
            遥测控制 <span className="dim">（即将支持，当前仅保存）</span>
          </div>
          <label className="check">
            <input type="checkbox" disabled /> 关闭 Sentry 崩溃上报
          </label>
          <label className="check">
            <input type="checkbox" disabled /> 关闭 Statsig 分析
          </label>
          <label className="check">
            <input type="checkbox" disabled /> 关闭 Sparkle 自动更新
          </label>
        </div>

        <div className="modal-actions">
          <button onClick={onCancel}>取消</button>
          <button className="primary" onClick={submit}>
            保存
          </button>
        </div>
      </div>
    </div>
  );
}
