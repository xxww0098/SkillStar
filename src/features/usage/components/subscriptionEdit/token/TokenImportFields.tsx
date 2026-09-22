import { useTranslation } from "react-i18next";
import { Textarea } from "@/components/ui/textarea";
import { Field } from "../fields";

interface TokenImportFieldsProps {
  providerName: string;
  token: string;
  setToken: (value: string) => void;
}

/**
 * Token-import paste. The value is sent only to `import_subscription_token`.
 * It is not an API key and is not kept in a DTO.
 */
export function TokenImportFields({ providerName, token, setToken }: TokenImportFieldsProps) {
  const { t } = useTranslation();

  return (
    <Field label={t("usage.fieldTokenImport")} hint={t("usage.tokenImportHint", { provider: providerName })}>
      <Textarea
        value={token}
        onChange={(event) => setToken(event.target.value)}
        placeholder={t("usage.tokenImportPlaceholder")}
        rows={4}
        spellCheck={false}
        autoComplete="off"
        aria-label={t("usage.fieldTokenImport")}
        className="min-h-20 resize-y font-mono text-[11px] leading-relaxed"
      />
    </Field>
  );
}
