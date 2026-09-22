import { useTranslation } from "react-i18next";
import { cn } from "@/lib/utils";
import { Field } from "../fields";

export type KiroLoginLeg = "portal" | "idc";

/** Portal PKCE first; AWS IDC device flow second. */
export function KiroLoginLegPicker({
  value,
  onChange,
}: {
  value: KiroLoginLeg;
  onChange: (leg: KiroLoginLeg) => void;
}) {
  const { t } = useTranslation();
  const options: Array<[KiroLoginLeg, string]> = [
    ["portal", t("usage.kiroLoginPortal")],
    ["idc", t("usage.kiroLoginIdc")],
  ];
  return (
    <Field label={t("usage.kiroLoginLeg")}>
      <div className="flex gap-1.5 rounded-xl border border-border bg-muted/50 p-1">
        {options.map(([id, label]) => (
          <button
            key={id}
            type="button"
            onClick={() => onChange(id)}
            className={cn(
              "flex-1 rounded-lg border px-2.5 py-1.5 text-[11px] font-semibold transition-all duration-200",
              value === id
                ? "border-border bg-background text-foreground shadow-sm"
                : "border-transparent text-muted-foreground hover:text-foreground",
            )}
          >
            {label}
          </button>
        ))}
      </div>
    </Field>
  );
}
