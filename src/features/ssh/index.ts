/** Public API barrel for the SSH remote feature. */
export { sshKeys } from "./api/keys";
export { useHostMutations, useSshHostsQuery } from "./api/hosts";
export {
  useAcceptHostKey,
  useDeleteRemoteSkill,
  usePushSkill,
  useRemoteSkillsQuery,
  useTestConnection,
} from "./api/remote";
export { SshHostsList } from "./components/SshHostsList";
export { SshHostForm } from "./components/SshHostForm";
export type { SshHostFormValues } from "./components/SshHostForm";
export { RemoteSkillPanel } from "./components/RemoteSkillPanel";
export { ConnectionConsole } from "./components/ConnectionConsole";
export { useConnectStream } from "./hooks/useConnectStream";
export type { SshProgressLine, PendingHostKey } from "./hooks/useConnectStream";
