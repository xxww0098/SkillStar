/** Query-key factory for the settings feature. Every TanStack Query key in
 * this feature must come from here so invalidation stays consistent. */
export const settingsKeys = {
  all: ["settings"] as const,
  gitStatus: () => [...settingsKeys.all, "git-status"] as const,
  acpConfig: () => [...settingsKeys.all, "acp-config"] as const,
  githubMirrorPresets: () => [...settingsKeys.all, "github-mirror-presets"] as const,
};
