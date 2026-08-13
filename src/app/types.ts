export type View = "overview" | "connections" | "access" | "tools" | "audit" | "settings";
export type SettingsTab = "general" | "about";
export type ThemeMode = "system" | "light" | "dark";
export type EffectiveTheme = "light" | "dark";
export type ToastTone = "success" | "error" | "info";

export type AuditFilters = {
  from: string;
  to: string;
  tool: string;
  connection: string;
  status: string;
  token: string;
};

export interface ToastMessage {
  id: string;
  message: string;
  tone: ToastTone;
  leaving?: boolean;
}
