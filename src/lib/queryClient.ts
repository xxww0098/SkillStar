import { QueryClient } from "@tanstack/react-query";

export const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      // Desktop: opening a file picker, OAuth window or another app blurs the
      // Tauri webview. Refetch-on-focus turned every blur into a full skills /
      // marketplace / MCP / models round trip. Explicit refresh buttons and
      // per-feature intervals still exist; 60s staleTime covers "I just did
      // something elsewhere".
      staleTime: 60_000,
      gcTime: 10 * 60 * 1000,
      refetchOnWindowFocus: false,
      retry: 1,
    },
  },
});
