import { invoke } from "@tauri-apps/api/core";
import type {
  AppDetection,
  ConnectionInfo,
  LaunchResult,
  Profile,
  TestReport,
} from "./types";

export const listProfiles = () => invoke<Profile[]>("list_profiles");
export const saveProfile = (profile: Profile) =>
  invoke<Profile>("save_profile", { profile });
export const deleteProfile = (id: string) =>
  invoke<void>("delete_profile", { id });
export const duplicateProfile = (id: string) =>
  invoke<Profile>("duplicate_profile", { id });
export const testConnection = (id: string) =>
  invoke<TestReport>("test_connection", { id });
export const applyDetected = (id: string, timezone: string, language: string) =>
  invoke<Profile>("apply_detected", { id, timezone, language });
export const launch = (id: string, diagnosticMode: boolean) =>
  invoke<LaunchResult>("launch", { id, diagnosticMode });
export const detectApp = () => invoke<AppDetection>("detect_app");
export const observeConnections = (pid: number) =>
  invoke<ConnectionInfo[]>("observe_connections", { pid });
