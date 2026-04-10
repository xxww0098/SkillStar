import { invoke } from "@tauri-apps/api/core";
import { useCallback, useRef, useState } from "react";
import { toast } from "sonner";
import { openExternalUrl } from "../../../lib/externalOpen";
import type { ProviderEntry } from "./useModelProviders";

interface GeminiOAuthCompletePayload {
  email: string;
  accessToken: string;
  refreshToken?: string | null;
}

export function useGeminiOAuth({ onAccountAdded }: { onAccountAdded?: (provider: ProviderEntry) => void }) {
  const [oauthLoading, setOauthLoading] = useState(false);
  const activeLoginId = useRef<string | null>(null);

  const startOAuth = useCallback(async () => {
    if (oauthLoading) return;
    setOauthLoading(true);

    try {
      const result = await invoke<{ loginId: string; verificationUri: string }>("gemini_oauth_start");
      activeLoginId.current = result.loginId;
      const opened = await openExternalUrl(result.verificationUri);
      if (!opened) {
        throw new Error("Failed to open browser for Gemini OAuth");
      }
      toast.info("请在浏览器中完成 Google 登录授权");

      const payload = await invoke<GeminiOAuthCompletePayload>("gemini_oauth_complete", { loginId: result.loginId });

      if (onAccountAdded) {
        const provider: ProviderEntry = {
          id: `gemini_oauth_${Date.now()}`,
          name: payload.email,
          category: "official",
          settingsConfig: {
            env: {
              GEMINI_API_KEY: payload.accessToken,
              GEMINI_REFRESH_TOKEN: payload.refreshToken ?? undefined,
              GEMINI_ACCOUNT_EMAIL: payload.email,
            },
          },
        };
        onAccountAdded(provider);
      }

      toast.success(`Google 账号授权成功: ${payload.email}`);
    } catch (e) {
      if (activeLoginId.current !== null) {
        toast.error(`授权失败: ${String(e)}`);
      }
    } finally {
      setOauthLoading(false);
      activeLoginId.current = null;
    }
  }, [oauthLoading, onAccountAdded]);

  const cancelOAuth = useCallback(async () => {
    try {
      await invoke("gemini_oauth_cancel", { loginId: activeLoginId.current });
    } catch {}
    setOauthLoading(false);
    activeLoginId.current = null;
  }, []);

  return { oauthLoading, startOAuth, cancelOAuth };
}
