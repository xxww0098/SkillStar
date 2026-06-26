/** S3 cloud sync feature: target config (Settings) + Cloud scope (My Skills). */
export { CloudSkillsContent } from "./components/CloudSkillPanel";
export { S3TargetForm, type S3TargetFormValues } from "./components/S3TargetForm";
export {
  useAddS3Target,
  useDeleteS3Target,
  useS3TargetsQuery,
  useTestS3Connection,
  useUpdateS3Target,
} from "./api/targets";
export { useCloudManifestQuery, useInstallFromCloud, usePushToCloud } from "./api/sync";
