/** Query-key factory for the marketplace feature's publisher-detail drill-down
 * (`PublisherDetail`). `useMarketplace.ts` still derives its own search /
 * leaderboard / publishers keys inline — left as-is to keep this factory
 * scoped to the local-first + stale-refresh flow being migrated here. */
export const marketplaceKeys = {
  all: ["marketplace"] as const,

  // Publisher detail drill-down: repos for one publisher, then skills for one
  // repo within that publisher (`PublisherDetail`).
  publisherDetail: () => [...marketplaceKeys.all, "publisher-detail"] as const,
  publisherRepos: (publisherName: string) => [...marketplaceKeys.publisherDetail(), "repos", publisherName] as const,
  repoSkills: (source: string) => [...marketplaceKeys.publisherDetail(), "repo-skills", source] as const,
};
