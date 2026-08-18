export type ProxyProtocol = "socks5" | "http" | "https";

export interface ProxyConfig {
  protocol: ProxyProtocol;
  host: string;
  port: number;
  username?: string | null;
  password?: string | null;
}

export interface TelemetryToggles {
  disableSentry: boolean;
  disableStatsig: boolean;
  disableSparkle: boolean;
}

export interface InjectionOptions {
  /** 注入时区（TZ env + Windows --timezone-for-testing）。默认关。 */
  injectTimezone: boolean;
  /** 注入语言（--lang / --accept-lang + LANG / LC_ALL）。默认关。 */
  injectLanguage: boolean;
  /** WebRTC / QUIC / DNS 泄露防护。默认开。 */
  leakProtection: boolean;
  /** 同步代理到 ~/.codex/.env。默认开。 */
  syncCodexEnv: boolean;
}

export interface Profile {
  id: string;
  name: string;
  proxy: ProxyConfig;
  /** IANA timezone; null = auto-detect via proxy at launch. */
  timezone?: string | null;
  /** BCP-47 locale; null = auto-detect via proxy at launch. */
  language?: string | null;
  telemetry: TelemetryToggles;
  injection: InjectionOptions;
  /** Manual override of the app binary path; null = auto-detect. */
  appPath?: string | null;
  createdAt: number;
  updatedAt: number;
}

export interface ExitInfo {
  ip: string;
  country: string;
  region: string;
  city: string;
  timezone: string;
  source: string;
}

export interface ConsistencyResult {
  ok: boolean;
  warnings: string[];
}

export interface TestReport {
  exit: ExitInfo;
  consistency: ConsistencyResult;
}

export interface ConnectionInfo {
  local: string;
  remote: string;
  state: string;
  pid?: number | null;
  process?: string | null;
}

export interface CdpTarget {
  id: string;
  kind: string;
  title: string;
  url: string;
  wsUrl: string;
}

export interface WebRtcCandidate {
  ip: string;
  kind: string;
}

export interface WebRtcCheck {
  candidates: WebRtcCandidate[];
  leaked: boolean;
  note?: string | null;
}

export interface AppProbe {
  reachable: boolean;
  browser?: string | null;
  targets: CdpTarget[];
  timezone?: string | null;
  language?: string | null;
  localTime?: string | null;
  exit?: ExitInfo | null;
  consistency?: ConsistencyResult | null;
  webrtc?: WebRtcCheck | null;
  hints: string[];
}

export interface LaunchResult {
  pid: number;
  appPath: string;
  proxyUrl: string;
  timezone?: string | null;
  language?: string | null;
  diagnosticMode: boolean;
  debugPort?: number | null;
  exitInfo?: ExitInfo | null;
  codexEnvSynced?: boolean;
  codexEnvNote?: string | null;
}

export interface AppDetection {
  path?: string | null;
  source: string;
  message: string;
}
