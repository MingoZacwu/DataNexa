import { AlertTriangle, ArrowUpRight, ChevronDown, FolderOpen, Github, LifeBuoy, Power, RotateCcw, X } from "lucide-react";
import { useState } from "react";
import { WindowControls, WindowDragRegion } from "../../components/chrome";
import { formatMessage, type I18nMessages } from "../../i18n";
import { api, isTauri } from "../../lib/tauri";
import type { ConfigLoadFailure } from "../../types";

/** Full-screen recovery screen shown instead of the workspace when config.toml
 * could not be parsed. It never mounts the app shell, so no background polling
 * or workspace request runs behind it. */
export function ConfigErrorPage({ t, error }: { t: I18nMessages; error: ConfigLoadFailure }) {
  const isMacos = typeof navigator !== "undefined" && /Mac|iPhone|iPad|iPod/i.test(navigator.userAgent);
  const [detailsOpen, setDetailsOpen] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  // Set once the config on disk is valid again but the automatic relaunch did
  // not take. The reset action must then disappear: invoking it again would be
  // rejected because the failure flag is already cleared.
  const [repaired, setRepaired] = useState(false);

  async function handleReset() {
    if (!confirming) {
      setConfirming(true);
      setFailure(null);
      return;
    }
    setBusy(true);
    setFailure(null);
    let restored: string;
    try {
      const outcome = await api.resetCorruptConfig();
      restored = outcome.backup_path
        ? formatMessage(t.bootError.resetDoneWithBackup, { path: outcome.backup_path })
        : t.bootError.resetDone;
      setNotice(restored);
    } catch (error) {
      setFailure(formatMessage(t.bootError.resetFailed, { message: error instanceof Error ? error.message : String(error) }));
      setConfirming(false);
      setBusy(false);
      return;
    }

    // The configuration is already repaired at this point; a relaunch failure
    // must not be reported as a failed reset.
    try {
      if (isTauri) {
        await api.restartApp();
      } else {
        window.setTimeout(() => window.location.reload(), 600);
      }
    } catch {
      setFailure(`${restored} ${t.bootError.restartHint}`);
      setRepaired(true);
      setConfirming(false);
      setBusy(false);
    }
  }

  async function handleOpenFolder() {
    try {
      await api.openConfigDirectory();
    } catch (error) {
      setFailure(formatMessage(t.bootError.openFolderFailed, { message: error instanceof Error ? error.message : String(error) }));
    }
  }

  return (
    <div className="config-error-page">
      <div className="ambient-grid" aria-hidden="true" />
      <WindowDragRegion />
      {!isMacos && <WindowControls t={t} />}

      <div className="config-error-frame" role="alert">
        <aside className="config-error-visual" aria-hidden="true">
          <div className="config-error-file">
            <span className="config-error-file-fold" />
            <span className="config-error-file-scan" />
            <span className="config-error-file-rows">
              <span className="config-error-row is-title" />
              <span className="config-error-row is-long" />
              <span className="config-error-row is-indent" />
              <span className="config-error-row is-broken">
                <span className="config-error-break-left" />
                <span className="config-error-break-right" />
                <span className="config-error-fragment" />
              </span>
              <span className="config-error-row is-indent" />
              <span className="config-error-row is-medium" />
              <span className="config-error-row is-short" />
            </span>
          </div>
        </aside>

        <section className="config-error-body">
          <div className="config-error-content">
            <header className="config-error-heading">
              <h1>{t.bootError.title}</h1>
              <p>{t.bootError.description}</p>
              <p>{t.bootError.descriptionSecondary}</p>
            </header>

            <div className="config-error-path">
              <span>{t.bootError.pathLabel}</span>
              <code title={error.path}>{error.path}</code>
            </div>

            <button
              type="button"
              className="config-error-details-toggle"
              onClick={() => setDetailsOpen((value) => !value)}
              aria-expanded={detailsOpen}
            >
              <ChevronDown size={14} className={detailsOpen ? "is-open" : undefined} />
              {detailsOpen ? t.bootError.detailsHide : t.bootError.detailsToggle}
            </button>
            {detailsOpen && <pre className="config-error-details">{error.message}</pre>}

            {confirming && <p className="config-error-confirm">{t.bootError.resetDescription}</p>}

            {failure && <p className="config-error-failure">{failure}</p>}
            {notice && <p className="config-error-notice">{notice}</p>}
            {busy && <p className="config-error-hint">{t.bootError.restartHint}</p>}
          </div>

          <div className="config-error-actions">
            {!repaired && (
              <button type="button" className="button danger-solid" disabled={busy} onClick={() => void handleReset()}>
                {confirming ? <AlertTriangle size={15} /> : <RotateCcw size={15} />}
                {confirming ? t.bootError.resetConfirm : t.bootError.resetTitle}
              </button>
            )}
            <div className="config-error-actions-pair">
              {confirming ? (
                <button type="button" className="button ghost" disabled={busy} onClick={() => setConfirming(false)}>
                  <X size={15} />
                  {t.bootError.resetCancel}
                </button>
              ) : (
                <button type="button" className="button ghost" disabled={busy} onClick={() => void handleOpenFolder()}>
                  <FolderOpen size={15} />
                  {t.bootError.openFolder}
                </button>
              )}
              <button type="button" className="button ghost" disabled={busy} onClick={() => void api.exitApp().catch(() => undefined)}>
                <Power size={15} />
                {t.bootError.quit}
              </button>
            </div>
          </div>

          <footer className="config-error-links">
            <button type="button" className="link-button" onClick={() => void api.openProjectHelp("faq").catch(() => undefined)}>
              <LifeBuoy size={14} />
              {t.bootError.help}
            </button>
            <button type="button" className="link-button" onClick={() => void api.openProjectIssues().catch(() => undefined)}>
              <Github size={14} />
              {t.bootError.issues}
            </button>
          </footer>
        </section>
      </div>
    </div>
  );
}
