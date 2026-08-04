/** Public API barrel for the SSH remote feature. */
export { sshKeys } from "./api/keys";
export { useHostMutations, useImportSystemHost, useSshHostsQuery } from "./api/hosts";
export {
  useAcceptHostKey,
  useDeleteRemoteSkill,
  useDiscoverRemoteSkillsQuery,
  useMigrateRemoteSkill,
  usePushSkill,
  useRemoteSkillsQuery,
  useTestConnection,
} from "./api/remote";
export { SshHostForm } from "./components/SshHostForm";
export type { SshHostFormValues } from "./components/SshHostForm";
export { useConnectStream } from "./hooks/useConnectStream";
export type { PendingHostKey, SshProgressLine } from "./hooks/useConnectStream";
