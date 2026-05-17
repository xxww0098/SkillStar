import type {
  ImportBundleResult,
  ImportMultiBundleResult,
  RepoNewSkill,
  ShareCodeInstallSummary,
  ShareCodeSkillInput,
  Skill,
  SkillCardDeck,
  SkillContent,
  SkillUpdateState,
  UpdateResult,
} from "../../../types";

/** Installed-skill lifecycle, ghost (unseen repo) skills, and deck group ops. */
export interface SkillCommands {
  // Installed skill lifecycle
  list_skills: { args: Record<string, never>; result: Skill[] };
  refresh_skill_updates: { args: { names?: string[] }; result: SkillUpdateState[] };
  install_skill: { args: { url: string; name?: string }; result: Skill };
  uninstall_skill: { args: { name: string }; result: void };
  update_skill: { args: { name: string }; result: UpdateResult };

  // Skill content (editor)
  read_skill_file_raw: { args: { name: string }; result: string };
  read_skill_content: { args: { name: string }; result: SkillContent };
  update_skill_content: { args: { name: string; content: string }; result: void };
  list_skill_files: { args: { name: string }; result: string[] };

  // Local-authored skills
  create_local_skill_from_content: { args: { name: string; content: string }; result: void };
  create_local_skill: { args: { name: string; content?: string }; result: Skill };
  delete_local_skill: { args: { name: string }; result: void };
  migrate_local_skills: { args: Record<string, never>; result: number };

  // Batch maintenance
  clean_broken_skills: { args: Record<string, never>; result: number };
  ai_batch_process_skills: { args: { skillNames: string[] }; result: void };

  // Share-code installer
  install_from_share_code: { args: { skills: ShareCodeSkillInput[] }; result: ShareCodeInstallSummary };

  // Bundles (.ags / .agd)
  export_skill_bundle: { args: { name: string; outputPath?: string }; result: string };
  preview_skill_bundle: { args: { filePath: string }; result: import("../../../types").BundleManifest };
  import_skill_bundle: { args: { filePath: string; force: boolean }; result: ImportBundleResult };
  export_multi_skill_bundle: { args: { names: string[]; outputPath: string }; result: string };
  preview_multi_skill_bundle: { args: { filePath: string }; result: import("../../../types").MultiManifest };
  import_multi_skill_bundle: { args: { filePath: string; force: boolean }; result: ImportMultiBundleResult };

  // Ghost (new repo skills) queue
  check_new_repo_skills: { args: Record<string, never>; result: RepoNewSkill[] };
  get_dismissed_new_skills: { args: Record<string, never>; result: string[] };
  dismiss_new_skill: { args: { key: string }; result: void };
  dismiss_new_skills_batch: { args: { keys: string[] }; result: void };

  // Local folder adoption
  adopt_local_folder: { args: { folderPath: string }; result: { adopted: { name: string }[] } };

  // Skill decks / groups
  list_skill_groups: { args: Record<string, never>; result: SkillCardDeck[] };
  create_skill_group: {
    args: {
      name: string;
      description: string;
      icon: string;
      skills: string[];
      skillSources?: Record<string, string>;
    };
    result: SkillCardDeck;
  };
  update_skill_group: {
    args: {
      id: string;
      name?: string;
      description?: string;
      icon?: string;
      skills?: string[];
      skillSources?: Record<string, string>;
    };
    result: SkillCardDeck;
  };
  delete_skill_group: { args: { id: string }; result: void };
  duplicate_skill_group: { args: { id: string }; result: SkillCardDeck };
  deploy_skill_group: {
    args: { groupId: string; projectPath: string; agentTypes: string[] };
    result: number;
  };
}
