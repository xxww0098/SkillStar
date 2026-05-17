import { createContext, useContext, type ReactNode } from "react";
import { useUsageData } from "../hooks/useUsageData";

type UsageDataContextValue = ReturnType<typeof useUsageData>;

const UsageDataContext = createContext<UsageDataContextValue | null>(null);

export function UsageDataProvider({ children }: { children: ReactNode }) {
  const value = useUsageData();
  return <UsageDataContext.Provider value={value}>{children}</UsageDataContext.Provider>;
}

/** Usage page + sidebar nav share one data source while Usage mode is active. */
export function useUsageDataContext(): UsageDataContextValue {
  const ctx = useContext(UsageDataContext);
  if (!ctx) {
    throw new Error("useUsageDataContext must be used within UsageDataProvider");
  }
  return ctx;
}
