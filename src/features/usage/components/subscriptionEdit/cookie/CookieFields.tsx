import { useTranslation } from "react-i18next";
import { Textarea } from "../../../../../components/ui/textarea";
import type { CatalogEntry, Subscription } from "../../../types";
import { Field } from "../fields";

interface CookieFieldsProps {
  editing: Subscription | null;
  selectedEntry: CatalogEntry;
  cookieHeader: string;
  setCookieHeader: (value: string) => void;
}

/**
 * Cookie-mode credential input: the raw `Cookie:` header copied out of the
 * provider console's DevTools. The backend parses and encrypts it into
 * `cookie_jar_encrypted`; nothing is stored in the browser.
 *
 * The per-provider "which cookie, from where" guidance is not repeated here —
 * `catalog.rs` already carries it as the entry's `warning`, which the dialog
 * renders directly above this field.
 */
export function CookieFields({ editing, selectedEntry, cookieHeader, setCookieHeader }: CookieFieldsProps) {
  const { t } = useTranslation();
  const configured = Boolean(editing?.has_credential);

  return (
    <Field
      label={configured ? t("usage.fieldCookieOptional") : t("usage.fieldCookie")}
      hint={t("usage.cookieHint", { url: hostOf(selectedEntry.subscription_url) })}
    >
      <div className="space-y-2">
        <Textarea
          value={cookieHeader}
          onChange={(e) => setCookieHeader(e.target.value)}
          placeholder={t("usage.cookiePlaceholder")}
          rows={3}
          spellCheck={false}
          autoComplete="off"
          aria-label={t("usage.fieldCookie")}
          className="min-h-16 resize-y font-mono text-[11px] leading-relaxed"
        />
        {configured && <p className="text-[9px] text-muted-foreground">{t("usage.cookieConfigured")}</p>}
      </div>
    </Field>
  );
}

/** Bare host of the provider console URL, for the "open DevTools on …" hint. */
function hostOf(url: string): string {
  try {
    return new URL(url).host;
  } catch {
    return url;
  }
}
