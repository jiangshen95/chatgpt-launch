import type { Profile } from "../types";

interface Props {
  profile: Profile;
  busy: boolean;
  onLaunch: () => void;
  onTest: () => void;
  onEdit: () => void;
  onDuplicate: () => void;
  onDelete: () => void;
}

export function ProfileCard({
  profile,
  busy,
  onLaunch,
  onTest,
  onEdit,
  onDuplicate,
  onDelete,
}: Props) {
  const proxyLabel = `${profile.proxy.protocol}://${profile.proxy.host}:${profile.proxy.port}`;
  return (
    <div className="card">
      <div className="card-head">
        <span className="card-name">{profile.name || "未命名配置"}</span>
        <span className="card-badge">{profile.proxy.protocol.toUpperCase()}</span>
      </div>

      <div className="card-row">
        <span className="k">代理</span>
        <span className="v mono">{proxyLabel}</span>
      </div>
      <div className="card-row">
        <span className="k">时区</span>
        <span className="v">{profile.timezone ?? <em>自动</em>}</span>
      </div>
      <div className="card-row">
        <span className="k">语言</span>
        <span className="v">{profile.language ?? <em>自动</em>}</span>
      </div>
      <div className="card-row">
        <span className="k">遥测</span>
        <span className="v dim">Sentry / Statsig / Sparkle（即将支持）</span>
      </div>

      <div className="card-actions">
        <button className="primary" onClick={onLaunch} disabled={busy}>
          {busy ? "启动中…" : "启动"}
        </button>
        <button onClick={onTest} disabled={busy}>
          测试
        </button>
        <button onClick={onEdit} disabled={busy}>
          编辑
        </button>
        <button onClick={onDuplicate} disabled={busy}>
          复制
        </button>
        <button className="danger" onClick={onDelete} disabled={busy}>
          删除
        </button>
      </div>
    </div>
  );
}
