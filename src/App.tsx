import { useEffect, useRef, useState } from "react";
import {
  applyDetected,
  deleteProfile,
  detectApp,
  duplicateProfile,
  launch,
  listProfiles,
  observeConnections,
  saveProfile,
  testConnection,
} from "./api";
import { newBlankProfile } from "./constants";
import { ProfileCard } from "./components/ProfileCard";
import { ProfileForm } from "./components/ProfileForm";
import type {
  AppDetection,
  ConnectionInfo,
  LaunchResult,
  Profile,
  TestReport,
} from "./types";
import "./App.css";

function proxyInUse(conns: ConnectionInfo[], port: number): boolean {
  const v4 = `127.0.0.1:${port}`;
  const v6 = `[::1]:${port}`;
  return conns.some(
    (c) => c.remote === v4 || c.remote === v6 || c.remote === `localhost:${port}`,
  );
}

export default function App() {
  const [profiles, setProfiles] = useState<Profile[]>([]);
  const [detection, setDetection] = useState<AppDetection | null>(null);
  const [globalDiagnostic, setGlobalDiagnostic] = useState(false);

  const [formOpen, setFormOpen] = useState(false);
  const [editing, setEditing] = useState<Profile | null>(null);

  const [busyId, setBusyId] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const [testState, setTestState] = useState<{
    profile: Profile;
    report: TestReport | null;
    loading: boolean;
  } | null>(null);

  const [lastLaunch, setLastLaunch] = useState<{
    result: LaunchResult;
    profile: Profile;
  } | null>(null);
  const [conns, setConns] = useState<ConnectionInfo[] | null>(null);

  const noticeTimer = useRef<number | null>(null);
  const notify = (msg: string) => {
    setNotice(msg);
    if (noticeTimer.current) window.clearTimeout(noticeTimer.current);
    noticeTimer.current = window.setTimeout(() => setNotice(null), 4000);
  };

  const refresh = async () => {
    try {
      setProfiles(await listProfiles());
    } catch (e) {
      notify(String(e));
    }
  };

  useEffect(() => {
    refresh();
    detectApp()
      .then(setDetection)
      .catch((e) => notify(`检测应用失败: ${e}`));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const openNew = () => {
    setEditing(newBlankProfile());
    setFormOpen(true);
  };
  const openEdit = (p: Profile) => {
    setEditing(p);
    setFormOpen(true);
  };

  const handleSave = async (p: Profile) => {
    try {
      await saveProfile(p);
      setFormOpen(false);
      notify("已保存");
      await refresh();
    } catch (e) {
      notify(`保存失败: ${e}`);
    }
  };

  const handleDelete = async (p: Profile) => {
    if (!window.confirm(`确认删除「${p.name || "未命名配置"}」？`)) return;
    try {
      await deleteProfile(p.id);
      notify("已删除");
      await refresh();
    } catch (e) {
      notify(`删除失败: ${e}`);
    }
  };

  const handleDuplicate = async (p: Profile) => {
    try {
      await duplicateProfile(p.id);
      notify("已复制");
      await refresh();
    } catch (e) {
      notify(`复制失败: ${e}`);
    }
  };

  const handleLaunch = async (p: Profile) => {
    setBusyId(p.id);
    setConns(null);
    try {
      const result = await launch(p.id, globalDiagnostic);
      setLastLaunch({ result, profile: p });
      notify(
        result.diagnosticMode
          ? `已启动（诊断模式，调试端口 ${result.debugPort}，仅本地）`
          : `已启动，PID ${result.pid}`,
      );
    } catch (e) {
      notify(`启动失败: ${e}`);
    } finally {
      setBusyId(null);
    }
  };

  const handleTest = async (p: Profile) => {
    setTestState({ profile: p, report: null, loading: true });
    try {
      const report = await testConnection(p.id);
      setTestState({ profile: p, report, loading: false });
    } catch (e) {
      notify(`测试失败: ${e}`);
      setTestState(null);
    }
  };

  const handleApplyDetected = async (timezone: string, language: string) => {
    if (!testState) return;
    try {
      await applyDetected(testState.profile.id, timezone, language);
      notify("已应用检测结果");
      await refresh();
      setTestState(null);
    } catch (e) {
      notify(`应用失败: ${e}`);
    }
  };

  const handleObserve = async () => {
    if (!lastLaunch) return;
    try {
      const list = await observeConnections(lastLaunch.result.pid);
      setConns(list);
    } catch (e) {
      notify(`观测失败: ${e}`);
    }
  };

  return (
    <div className="app">
      <header className="topbar">
        <div className="brand">
          <h1>ChatGPT Launcher</h1>
          <span className="sub">为 ChatGPT 桌面版配置代理 / 时区 / 语言</span>
        </div>
        <div className="topbar-actions">
          <label className="check inline">
            <input
              type="checkbox"
              checked={globalDiagnostic}
              onChange={(e) => setGlobalDiagnostic(e.target.checked)}
            />
            诊断模式（默认关闭）
          </label>
          <button className="primary" onClick={openNew}>
            + 新建配置
          </button>
        </div>
      </header>

      <div className="detect-bar">
        {detection?.path ? (
          <>
            <span className="dot ok" /> ChatGPT App：{detection.path}
          </>
        ) : (
          <>
            <span className="dot warn" /> {detection?.message ?? "正在检测 ChatGPT App…"}
          </>
        )}
      </div>

      {notice && <div className="toast">{notice}</div>}

      <main className="grid">
        {profiles.length === 0 && (
          <div className="empty">
            <p>还没有配置。点击右上角「新建配置」，填入你的代理地址即可。</p>
          </div>
        )}
        {profiles.map((p) => (
          <ProfileCard
            key={p.id}
            profile={p}
            busy={busyId === p.id}
            onLaunch={() => handleLaunch(p)}
            onTest={() => handleTest(p)}
            onEdit={() => openEdit(p)}
            onDuplicate={() => handleDuplicate(p)}
            onDelete={() => handleDelete(p)}
          />
        ))}
      </main>

      {lastLaunch && (
        <section className="panel">
          <h3>最近一次启动</h3>
          <div className="panel-row">
            <span className="k">PID</span>
            <span className="v mono">{lastLaunch.result.pid}</span>
            <span className="k">应用</span>
            <span className="v mono">{lastLaunch.result.appPath}</span>
          </div>
          {lastLaunch.result.diagnosticMode && (
            <div className="panel-row">
              <span className="k">调试端口</span>
              <span className="v mono">127.0.0.1:{lastLaunch.result.debugPort}</span>
              <span className="v dim">仅用于诊断，日常请关闭</span>
            </div>
          )}
          <div className="panel-actions">
            <button onClick={handleObserve}>观测连接</button>
            {conns && (
              <span className="v">
                检测到 {conns.length} 条连接；代理{" "}
                {proxyInUse(conns, lastLaunch.profile.proxy.port) ? (
                  <strong className="ok-text">已生效 ✓</strong>
                ) : (
                  <strong className="warn-text">未命中本地代理端口</strong>
                )}
              </span>
            )}
          </div>
          {conns && conns.length > 0 && (
            <table className="conns">
              <thead>
                <tr>
                  <th>本地</th>
                  <th>远端</th>
                  <th>状态</th>
                </tr>
              </thead>
              <tbody>
                {conns.map((c, i) => (
                  <tr key={i}>
                    <td className="mono">{c.local}</td>
                    <td className="mono">{c.remote}</td>
                    <td>{c.state}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </section>
      )}

      {formOpen && (
        <ProfileForm
          initial={editing}
          onSave={handleSave}
          onCancel={() => setFormOpen(false)}
        />
      )}

      {testState && (
        <div className="overlay">
          <div className="modal">
            <h2>连接测试</h2>
            {testState.loading && <p>正在通过代理请求中立端点…</p>}
            {testState.report && (
              <>
                <div className="kv">
                  <div>
                    <span className="k">出口 IP</span>
                    <span className="v mono">{testState.report.exit.ip || "—"}</span>
                  </div>
                  <div>
                    <span className="k">位置</span>
                    <span className="v">
                      {testState.report.exit.country} {testState.report.exit.region}{" "}
                      {testState.report.exit.city}
                    </span>
                  </div>
                  <div>
                    <span className="k">时区</span>
                    <span className="v mono">{testState.report.exit.timezone || "—"}</span>
                  </div>
                  <div>
                    <span className="k">数据来源</span>
                    <span className="v">{testState.report.exit.source}</span>
                  </div>
                </div>

                {testState.report.consistency.ok ? (
                  <p className="ok-text">一致性检查通过，未发现 IP/时区/语言冲突。</p>
                ) : (
                  <ul className="warnings">
                    {testState.report.consistency.warnings.map((w, i) => (
                      <li key={i}>{w}</li>
                    ))}
                  </ul>
                )}

                {testState.report.exit.timezone && (
                  <div className="modal-actions">
                    <button onClick={() => setTestState(null)}>关闭</button>
                    <button
                      className="primary"
                      onClick={() => {
                        const lang = defaultLangForCountry(testState.report!.exit.country);
                        handleApplyDetected(
                          testState.report!.exit.timezone,
                          lang ?? testState.profile.language ?? "",
                        );
                      }}
                    >
                      应用检测结果（时区/语言）
                    </button>
                  </div>
                )}
              </>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

function defaultLangForCountry(country: string): string | null {
  const map: Record<string, string> = {
    US: "en-US",
    GB: "en-GB",
    CA: "en-CA",
    AU: "en-AU",
    NZ: "en-NZ",
    IE: "en-IE",
    JP: "ja-JP",
    KR: "ko-KR",
    CN: "zh-CN",
    TW: "zh-TW",
    HK: "zh-HK",
    DE: "de-DE",
    FR: "fr-FR",
    ES: "es-ES",
    MX: "es-MX",
    BR: "pt-BR",
    IT: "it-IT",
    NL: "nl-NL",
  };
  return map[country] ?? null;
}
