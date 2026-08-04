import { useEffect, useState } from "react";
import { PrototypeSwitcher } from "../../../../../components/shared/PrototypeSwitcher";
import { VariantD2 } from "./ia/VariantD2";
import { VariantD3 } from "./ia/VariantD3";
import type { ModelsNavBridge } from "./modelsNavBridge";
import { usePrototypeHub } from "./usePrototypeHub";

type AltVariant = "D2" | "D3";

const ALT_META = [
  { key: "D2", name: "App 竖切" },
  { key: "D3", name: "双栏 · Provider + 绑定" },
] as const;

function readAltVariant(): AltVariant {
  if (typeof window === "undefined") return "D2";
  const raw = new URLSearchParams(window.location.search).get("variant");
  return raw === "D3" ? "D3" : "D2";
}

/**
 * DEV-only alternate IA prototypes (D2 / D3).
 * Production Models hub is the D1 matrix via `ModelsHub` (`#models`).
 */
export function ModelsHubPrototype(nav: ModelsNavBridge) {
  const data = usePrototypeHub(nav);
  const [variant, setVariant] = useState<AltVariant>(readAltVariant);

  useEffect(() => {
    const sync = () => setVariant(readAltVariant());
    window.addEventListener("popstate", sync);
    window.addEventListener("prototype-variant-change", sync);
    return () => {
      window.removeEventListener("popstate", sync);
      window.removeEventListener("prototype-variant-change", sync);
    };
  }, []);

  useEffect(() => {
    data.closeOverlay();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [variant]);

  const setVariantParam = (key: string) => {
    const next: AltVariant = key === "D3" ? "D3" : "D2";
    const url = new URL(window.location.href);
    url.searchParams.set("variant", next);
    window.history.replaceState(null, "", `${url.pathname}${url.search}${url.hash}`);
    setVariant(next);
  };

  return (
    <>
      {variant === "D2" ? <VariantD2 data={data} /> : <VariantD3 data={data} />}
      <PrototypeSwitcher variants={[...ALT_META]} current={variant} onChange={setVariantParam} />
    </>
  );
}
