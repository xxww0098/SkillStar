/** Public API barrel for the My Skills feature — cross-feature imports go through here. */
export { LocalSkillsContent } from "./components/LocalSkillsContent";
export { MySkillsRemoteHostPicker } from "./components/MySkillsRemoteHostPicker";
export { MySkillsScopeSwitch, type MySkillsScope } from "./components/MySkillsScopeSwitch";
export { ScopeDetailDrawer, type ScopeDetailProps } from "./components/ScopeDetailDrawer";
export { useMySkillsRemoteHosts } from "./hooks/useMySkillsRemoteHosts";
export { useMySkillsScope } from "./hooks/useMySkillsScope";
