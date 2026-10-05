import { AccountsPanel } from "@/features/accounts";
import type { CatalogFilter } from "@/features/usage";

interface AccountsProps {
  filter: CatalogFilter;
  accountsCreateRequest: { nonce: number; preselectCatalogId: string | null } | null;
  clearAccountsCreateRequest: () => void;
}

export function Accounts({ filter, accountsCreateRequest, clearAccountsCreateRequest }: AccountsProps) {
  return (
    <div className="flex-1 min-w-0 flex flex-col overflow-hidden">
      <AccountsPanel
        filter={filter}
        accountsCreateRequest={accountsCreateRequest}
        clearAccountsCreateRequest={clearAccountsCreateRequest}
      />
    </div>
  );
}
