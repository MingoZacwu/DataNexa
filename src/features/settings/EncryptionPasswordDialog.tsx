import * as Dialog from "@radix-ui/react-dialog";
import { KeyRound, X } from "lucide-react";
import { useState } from "react";
import type { FormEvent } from "react";
import { Field } from "../../components/ui";
import { formatMessage, type I18nMessages } from "../../i18n";

/** Mirrors MIN_PASSWORD_LENGTH in src-tauri/src/transfer_crypto.rs; the backend re-checks it. */
export const MIN_PASSWORD_LENGTH = 8;

export function EncryptionPasswordDialog({
  t,
  mode,
  exportedAt,
  error,
  busy,
  onSubmit,
  onRequestPlaintext,
  onCancel
}: {
  t: I18nMessages;
  mode: "export" | "import";
  exportedAt?: string | null;
  error?: string | null;
  busy: boolean;
  onSubmit: (password: string) => void;
  onRequestPlaintext?: () => void;
  onCancel: () => void;
}) {
  const [password, setPassword] = useState("");
  const [confirmation, setConfirmation] = useState("");
  const [validationError, setValidationError] = useState<string | null>(null);
  const copy = t.connectionTransfer;

  function submit(event: FormEvent) {
    event.preventDefault();
    if (busy) return;
    if (!password) {
      setValidationError(copy.passwordRequired);
      return;
    }
    // Only exports know the password rules. An import accepts whatever the file was written
    // with, so a short password must not be rejected here.
    if (mode === "export") {
      if (password.length < MIN_PASSWORD_LENGTH) {
        setValidationError(formatMessage(copy.passwordTooShort, { min: MIN_PASSWORD_LENGTH }));
        return;
      }
      if (password !== confirmation) {
        setValidationError(copy.passwordMismatch);
        return;
      }
    }
    setValidationError(null);
    onSubmit(password);
  }

  const displayedError = validationError ?? error ?? "";
  const exportedTime = exportedAt ? new Date(exportedAt).toLocaleString() : null;

  return (
    <Dialog.Root open onOpenChange={(open) => !open && !busy && onCancel()}>
      <Dialog.Portal>
        <Dialog.Overlay className="dialog-overlay" />
        <Dialog.Content className="policy-dialog transfer-dialog">
          <div className="dialog-titlebar">
            <div>
              <Dialog.Title>{mode === "export" ? copy.encryptTitle : copy.decryptTitle}</Dialog.Title>
              <Dialog.Description>{mode === "export" ? copy.encryptDescription : copy.decryptDescription}</Dialog.Description>
            </div>
            <Dialog.Close asChild>
              <button type="button" className="icon-button" aria-label={t.common.close} disabled={busy}>
                <X size={18} />
              </button>
            </Dialog.Close>
          </div>

          <form className="transfer-password-form" onSubmit={submit}>
            {mode === "import" && exportedTime ? (
              <p className="transfer-password-meta">
                <KeyRound size={14} />
                {formatMessage(copy.exportedAt, { time: exportedTime })}
              </p>
            ) : null}

            <Field label={copy.password}>
              <input
                autoFocus
                type="password"
                value={password}
                disabled={busy}
                onChange={(event) => setPassword(event.target.value)}
              />
            </Field>

            {mode === "export" ? (
              <Field label={copy.passwordConfirm}>
                <input
                  type="password"
                  value={confirmation}
                  disabled={busy}
                  onChange={(event) => setConfirmation(event.target.value)}
                />
              </Field>
            ) : null}

            {mode === "export" ? (
              <p className="transfer-password-hint">
                {formatMessage(copy.passwordHint, { min: MIN_PASSWORD_LENGTH })}
              </p>
            ) : null}

            {displayedError ? <p className="transfer-password-error">{displayedError}</p> : null}

            <footer className="transfer-dialog-actions">
              {mode === "export" && onRequestPlaintext ? (
                <button type="button" className="button ghost transfer-plaintext-action" disabled={busy} onClick={onRequestPlaintext}>
                  {copy.exportWithoutEncryption}
                </button>
              ) : null}
              <button type="button" className="button ghost" disabled={busy} onClick={onCancel}>
                {t.common.cancel}
              </button>
              <button type="submit" className="button primary" disabled={busy}>
                {mode === "export" ? copy.encryptAndExport : copy.decryptAndImport}
              </button>
            </footer>
          </form>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
