import { batch, signal } from "nsh";
import { toast } from "../../shell/events";
import { ct, ctf } from "./channelsStrings";
import { scheduleRefresh } from "./state";
import {
  realAuthControl,
  realChannelsBridge,
  type ActiveModal,
  type ChannelListEntry,
  type ChannelMembershipSnapshot,
  type ChannelReview,
  type ChannelUpdateSnapshot,
  type ChannelsAuthControl,
  type ChannelsBridge,
  type DivergenceChoice,
  type GitHubOrgSummary,
  type InboxInvitation,
  type LocalChannelsSnapshot,
  type PublishPreview,
  type RegistrationCandidateRepo,
  type RegistrationScanPreview,
  type RollbackTargetItem,
  type SkillUpdateResolutionInput,
} from "./channelsTypes";

export * from "./channelsTypes";

export function createChannelsState(
  bridge: ChannelsBridge,
  authCtrl: ChannelsAuthControl = realAuthControl,
  onLocalSkillsChanged?: () => void | Promise<void>,
) {
  const snapshot = signal<LocalChannelsSnapshot>({ channels: [], subscriptions: [] });
  const inbox = signal<InboxInvitation[]>([]);
  const selectedRepoId = signal<number | null>(null);
  const loading = signal(false);
  const busyKey = signal<string | null>(null);
  const errorMessage = signal<string | null>(null);

  // Per-skill divergence choice state (`key = "${repoId}:${skillId}"`)
  const resolutions = signal<Record<string, DivergenceChoice>>({});

  // Modal state
  const modal = signal<ActiveModal>(null);
  const modalBusy = signal(false);
  const modalError = signal<string | null>(null);

  // Create channel form
  const orgs = signal<GitHubOrgSummary[]>([]);
  const createOrg = signal("");
  const createName = signal("");
  const createDesc = signal("");

  // Register existing form
  const registerOrgId = signal<number | null>(null);
  const registerCandidates = signal<RegistrationCandidateRepo[]>([]);
  const registerPreview = signal<RegistrationScanPreview | null>(null);

  // Review / subscribe form
  const reviewData = signal<ChannelReview | null>(null);
  const reviewSelected = signal<Record<string, boolean>>({});

  // Publish form
  const publishPreviewData = signal<PublishPreview | null>(null);
  const publishTitle = signal("");
  const publishNotes = signal("");

  // Members / invite form
  const membershipData = signal<ChannelMembershipSnapshot | null>(null);
  const inviteUsername = signal("");
  const inviteRole = signal<"subscriber" | "publisher">("subscriber");

  // Rollback form
  const rollbackTargets = signal<RollbackTargetItem[]>([]);
  const rollbackRevision = signal<number | null>(null);
  const rollbackDivergence = signal<"none" | "preserve" | "discard">("none");

  // Convert local / export forms
  const convertLocalName = signal("");
  const exportOutDir = signal("");

  function makeSessionId(): string {
    return `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
  }

  function handleError(err: unknown): string {
    const raw = err instanceof Error ? err.message : String(err);
    if (raw.startsWith("AUTH_REQUIRED:")) {
      const prev = authCtrl.auth();
      authCtrl.auth.set({ state: "expired", login: prev?.login ?? null });
      return raw.replace(/^AUTH_REQUIRED:\s*/, "");
    }
    return raw;
  }

  function combinedEntries(): ChannelListEntry[] {
    const snap = snapshot();
    const byId = new Map<number, ChannelListEntry>();
    for (const ch of snap.channels) {
      byId.set(ch.repository_id, {
        repositoryId: ch.repository_id,
        descriptor: ch,
        subscription: null,
        displayName: `${ch.owner}/${ch.name}`,
      });
    }
    for (const sub of snap.subscriptions) {
      const id = sub.view.repository_id;
      const existing = byId.get(id);
      if (existing !== undefined) {
        existing.subscription = sub;
      } else {
        byId.set(id, {
          repositoryId: id,
          descriptor: null,
          subscription: sub,
          displayName: ctf("ch.repoId", { id }),
        });
      }
    }
    return Array.from(byId.values());
  }

  function selectedEntry(): ChannelListEntry | null {
    const list = combinedEntries();
    if (list.length === 0) return null;
    const id = selectedRepoId();
    return list.find((e) => e.repositoryId === id) ?? list[0];
  }

  async function refreshLocalSnapshot(): Promise<void> {
    loading.set(true);
    try {
      const next = await bridge.snapshot();
      batch(() => {
        snapshot.set(next);
        errorMessage.set(null);
      });
      const list = combinedEntries();
      if (list.length > 0 && !list.some((e) => e.repositoryId === selectedRepoId())) {
        selectedRepoId.set(list[0].repositoryId);
      }
    } catch (err: unknown) {
      errorMessage.set(handleError(err));
    } finally {
      loading.set(false);
    }
  }

  async function refreshInbox(): Promise<void> {
    if (authCtrl.auth()?.state !== "connected") {
      inbox.set([]);
      return;
    }
    try {
      const items = await bridge.listInbox();
      inbox.set(items);
    } catch (err: unknown) {
      errorMessage.set(handleError(err));
    }
  }

  async function refreshAll(): Promise<void> {
    await refreshLocalSnapshot();
    await refreshInbox();
  }

  function notifyLocalSkillsChanged(): void {
    if (!onLocalSkillsChanged) return;
    try {
      void Promise.resolve(onLocalSkillsChanged()).catch(() => {});
    } catch {
      // Ignore refresh notification failure so channel actions keep their own outcome.
    }
  }

  async function runBusy(key: string, fn: () => Promise<void>): Promise<void> {
    if (busyKey() !== null) return;
    busyKey.set(key);
    errorMessage.set(null);
    try {
      await fn();
    } catch (err: unknown) {
      const msg = handleError(err);
      errorMessage.set(msg);
      toast(msg, "error");
    } finally {
      busyKey.set(null);
    }
  }

  async function closeModal(): Promise<void> {
    const active = modal();
    if (active?.kind === "register" && registerPreview() !== null) {
      const sessionId = registerPreview()!.session_id;
      void bridge.cancelRegistration(sessionId).catch(() => {});
    }
    if (active?.kind === "publish" && publishPreviewData() !== null) {
      const sessionId = publishPreviewData()!.session_id;
      void bridge.cancelPublish(sessionId).catch(() => {});
    }
    batch(() => {
      modal.set(null);
      modalBusy.set(false);
      modalError.set(null);
      registerPreview.set(null);
      publishPreviewData.set(null);
    });
  }

  return {
    authCtrl,
    snapshot,
    inbox,
    selectedRepoId,
    loading,
    busyKey,
    errorMessage,
    resolutions,
    modal,
    modalBusy,
    modalError,
    orgs,
    createOrg,
    createName,
    createDesc,
    registerOrgId,
    registerCandidates,
    registerPreview,
    reviewData,
    reviewSelected,
    publishPreviewData,
    publishTitle,
    publishNotes,
    membershipData,
    inviteUsername,
    inviteRole,
    rollbackTargets,
    rollbackRevision,
    rollbackDivergence,
    convertLocalName,
    exportOutDir,

    combinedEntries,
    selectedEntry,
    refreshLocalSnapshot,
    refreshInbox,
    refreshAll,
    closeModal,

    selectChannel(repoId: number): void {
      selectedRepoId.set(repoId);
      errorMessage.set(null);
    },

    setResolution(repoId: number, skillId: string, choice: DivergenceChoice): void {
      resolutions.set({ ...resolutions(), [`${repoId}:${skillId}`]: choice });
    },

    getResolution(repoId: number, skillId: string): DivergenceChoice | undefined {
      return resolutions()[`${repoId}:${skillId}`];
    },

    async signOut(): Promise<void> {
      await runBusy("logout", async () => {
        await authCtrl.logout();
        await closeModal();
        inbox.set([]);
        orgs.set([]);
        await refreshLocalSnapshot();
      });
    },

    async refreshAuth(): Promise<void> {
      await runBusy("refresh-auth", async () => {
        await authCtrl.refreshAuth();
        await refreshAll();
      });
    },

    async toggleAutoUpdate(repoId: number, enabled: boolean): Promise<void> {
      await runBusy(`auto-update:${repoId}`, async () => {
        const res = await bridge.setAutoUpdate(repoId, enabled);
        await refreshLocalSnapshot();
        toast(
          ctf("ch.toastAutoUpdate", {
            state: res.enabled ? ct("ch.autoUpdateOn") : ct("ch.autoUpdateOff"),
          }),
          "success",
        );
      });
    },

    async acceptInvitation(invId: number): Promise<void> {
      await runBusy(`accept:${invId}`, async () => {
        const desc = await bridge.acceptInvitation(invId);
        selectedRepoId.set(desc.repository_id);
        await refreshAll();
        toast(ctf("ch.toastAccepted", { name: `${desc.owner}/${desc.name}` }), "success");
      });
    },

    async resumeAcceptedChannel(repoId: number): Promise<void> {
      await runBusy(`resume-accepted:${repoId}`, async () => {
        const desc = await bridge.resumeAcceptedChannel(repoId);
        await refreshLocalSnapshot();
        toast(ctf("ch.toastResumed", { name: `${desc.owner}/${desc.name}` }), "success");
      });
    },

    async declineInvitation(invId: number): Promise<void> {
      await runBusy(`decline:${invId}`, async () => {
        await bridge.declineInvitation(invId);
        await refreshInbox();
        toast(ctf("ch.toastDeclined", { id: invId }), "info");
      });
    },

    async resumePendingChannel(repoId: number): Promise<void> {
      await runBusy(`resume:${repoId}`, async () => {
        const desc = await bridge.resumeChannel(repoId);
        await refreshLocalSnapshot();
        toast(ctf("ch.toastResumed", { name: `${desc.owner}/${desc.name}` }), "success");
      });
    },

    async checkUpdates(repoId: number): Promise<void> {
      await runBusy(`check:${repoId}`, async () => {
        const snap = await bridge.checkUpdate(repoId);
        await refreshLocalSnapshot();
        toast(ctf("ch.toastChecked", { status: snap.status }), "info");
      });
    },

    async applyUpdates(repoId: number, updateSnap: ChannelUpdateSnapshot): Promise<void> {
      const blocked = updateSnap.items.filter((i) => i.state === "blocked");
      const currentRes = resolutions();
      const inputResolutions: SkillUpdateResolutionInput[] = [];
      for (const item of blocked) {
        const chosen = currentRes[`${repoId}:${item.id}`];
        if (chosen === undefined) {
          errorMessage.set(ct("ch.conflictNeedsChoice"));
          return;
        }
        inputResolutions.push({ skill_id: item.id, resolution: chosen });
      }
      await runBusy(`apply:${repoId}`, async () => {
        const res = await bridge.applyUpdate(repoId, updateSnap.target, inputResolutions);
        await refreshLocalSnapshot();
        notifyLocalSkillsChanged();
        toast(ctf("ch.toastApplied", { count: res.applied_skill_ids.length }), "success");
      });
    },

    async installChannelSkill(repoId: number, skillId: string): Promise<void> {
      await runBusy(`install:${repoId}:${skillId}`, async () => {
        await bridge.installChannelSkill(repoId, skillId);
        await refreshLocalSnapshot();
        notifyLocalSkillsChanged();
        toast(ctf("ch.toastInstalledSkill", { skill: skillId }), "success");
      });
    },

    async resumeFollowingSkill(repoId: number, skillId: string): Promise<void> {
      await runBusy(`unpin:${repoId}:${skillId}`, async () => {
        await bridge.resumeFollowingSkill(repoId, skillId);
        await refreshLocalSnapshot();
        toast(ctf("ch.toastUnpinned", { skill: skillId }), "success");
      });
    },

    async uninstallRemovedSkill(repoId: number, skillId: string, revoked: boolean): Promise<void> {
      await runBusy(`rm:${repoId}:${skillId}`, async () => {
        await bridge.uninstallRemovedSkill(repoId, skillId, revoked);
        await refreshLocalSnapshot();
        notifyLocalSkillsChanged();
        toast(ctf("ch.toastRemovedUninstalled", { skill: skillId }), "success");
      });
    },

    async openCreateModal(): Promise<void> {
      batch(() => {
        createName.set("");
        createDesc.set("");
        modalError.set(null);
        modal.set({ kind: "create" });
        modalBusy.set(true);
      });
      try {
        const list = await bridge.listOrganizations();
        batch(() => {
          orgs.set(list);
          createOrg.set(list[0]?.login ?? "");
        });
      } catch (err: unknown) {
        modalError.set(handleError(err));
      } finally {
        modalBusy.set(false);
      }
    },

    async submitCreate(): Promise<void> {
      const org = createOrg().trim();
      const name = createName().trim();
      if (org === "" || name === "") return;
      modalBusy.set(true);
      modalError.set(null);
      try {
        const desc = await bridge.createChannel(org, name, createDesc().trim());
        selectedRepoId.set(desc.repository_id);
        await closeModal();
        await refreshLocalSnapshot();
        toast(ctf("ch.toastCreated", { name: `${desc.owner}/${desc.name}` }), "success");
      } catch (err: unknown) {
        modalError.set(handleError(err));
      } finally {
        modalBusy.set(false);
      }
    },

    async openRegisterModal(): Promise<void> {
      batch(() => {
        registerCandidates.set([]);
        registerPreview.set(null);
        modalError.set(null);
        modal.set({ kind: "register" });
        modalBusy.set(true);
      });
      try {
        const list = await bridge.listOrganizations();
        batch(() => {
          orgs.set(list);
          registerOrgId.set(list[0]?.id ?? null);
        });
        if (list[0] !== undefined) {
          const candidates = await bridge.listRegistrationCandidates(list[0].id);
          registerCandidates.set(candidates);
        }
      } catch (err: unknown) {
        modalError.set(handleError(err));
      } finally {
        modalBusy.set(false);
      }
    },

    async selectRegisterOrg(orgId: number): Promise<void> {
      batch(() => {
        registerOrgId.set(orgId);
        registerPreview.set(null);
        modalError.set(null);
        modalBusy.set(true);
      });
      try {
        const candidates = await bridge.listRegistrationCandidates(orgId);
        registerCandidates.set(candidates);
      } catch (err: unknown) {
        modalError.set(handleError(err));
      } finally {
        modalBusy.set(false);
      }
    },

    async scanCandidate(orgId: number, repoId: number): Promise<void> {
      modalBusy.set(true);
      modalError.set(null);
      try {
        const preview = await bridge.scanExistingRepo(orgId, repoId, makeSessionId());
        registerPreview.set(preview);
      } catch (err: unknown) {
        modalError.set(handleError(err));
      } finally {
        modalBusy.set(false);
      }
    },

    async confirmRegister(): Promise<void> {
      const preview = registerPreview();
      if (preview === null) return;
      modalBusy.set(true);
      modalError.set(null);
      try {
        const desc = await bridge.confirmRegistration(preview.session_id);
        registerPreview.set(null);
        selectedRepoId.set(desc.repository_id);
        await closeModal();
        await refreshLocalSnapshot();
        toast(ctf("ch.toastRegistered", { name: `${desc.owner}/${desc.name}` }), "success");
      } catch (err: unknown) {
        modalError.set(handleError(err));
      } finally {
        modalBusy.set(false);
      }
    },

    async openReviewModal(repoId: number): Promise<void> {
      batch(() => {
        reviewData.set(null);
        reviewSelected.set({});
        modalError.set(null);
        modal.set({ kind: "review", repositoryId: repoId });
        modalBusy.set(true);
      });
      try {
        const review = await bridge.reviewChannel(repoId);
        const sel: Record<string, boolean> = {};
        for (const sk of review.skills) {
          if (sk.status !== "removed") sel[sk.id] = true;
        }
        batch(() => {
          reviewData.set(review);
          reviewSelected.set(sel);
        });
      } catch (err: unknown) {
        modalError.set(handleError(err));
      } finally {
        modalBusy.set(false);
      }
    },

    toggleReviewSkill(skillId: string): void {
      const next = { ...reviewSelected() };
      next[skillId] = !next[skillId];
      reviewSelected.set(next);
    },

    async confirmSubscribe(): Promise<void> {
      const rev = reviewData();
      const active = modal();
      if (rev === null || active?.kind !== "review") return;
      const selectedIds = rev.skills
        .filter((s) => s.status !== "removed" && reviewSelected()[s.id] === true)
        .map((s) => s.id);
      modalBusy.set(true);
      modalError.set(null);
      try {
        const sub = await bridge.subscribeChannel(active.repositoryId, rev.target, selectedIds);
        await closeModal();
        await refreshLocalSnapshot();
        notifyLocalSkillsChanged();
        toast(ctf("ch.toastSubscribed", { tag: sub.target.tag_name }), "success");
      } catch (err: unknown) {
        modalError.set(handleError(err));
      } finally {
        modalBusy.set(false);
      }
    },

    async openPublishModal(repoId: number): Promise<void> {
      batch(() => {
        publishPreviewData.set(null);
        publishTitle.set("");
        publishNotes.set("");
        modalError.set(null);
        modal.set({ kind: "publish", repositoryId: repoId });
        modalBusy.set(true);
      });
      try {
        const preview = await bridge.publishPreview(repoId, makeSessionId());
        batch(() => {
          publishPreviewData.set(preview);
          publishTitle.set(preview.tag_name);
        });
      } catch (err: unknown) {
        modalError.set(handleError(err));
      } finally {
        modalBusy.set(false);
      }
    },

    async confirmPublish(): Promise<void> {
      const preview = publishPreviewData();
      if (preview === null) return;
      const title = publishTitle().trim() || preview.tag_name;
      modalBusy.set(true);
      modalError.set(null);
      try {
        const res = await bridge.publishRelease(preview.session_id, title, publishNotes().trim());
        publishPreviewData.set(null);
        await closeModal();
        await refreshLocalSnapshot();
        toast(ctf("ch.toastPublished", { tag: res.manifest.tag_name }), "success");
      } catch (err: unknown) {
        modalError.set(handleError(err));
      } finally {
        modalBusy.set(false);
      }
    },

    async openMembersModal(repoId: number): Promise<void> {
      batch(() => {
        membershipData.set(null);
        inviteUsername.set("");
        inviteRole.set("subscriber");
        modalError.set(null);
        modal.set({ kind: "members", repositoryId: repoId });
        modalBusy.set(true);
      });
      try {
        membershipData.set(await bridge.listMembership(repoId));
      } catch (err: unknown) {
        modalError.set(handleError(err));
      } finally {
        modalBusy.set(false);
      }
    },

    async submitInvite(repoId: number): Promise<void> {
      const user = inviteUsername().trim();
      if (user === "") return;
      modalBusy.set(true);
      modalError.set(null);
      try {
        const res = await bridge.inviteMember(repoId, user, inviteRole());
        inviteUsername.set("");
        membershipData.set(await bridge.listMembership(repoId));
        toast(ctf("ch.toastInvited", { user: res.username, status: res.status }), "success");
      } catch (err: unknown) {
        modalError.set(handleError(err));
      } finally {
        modalBusy.set(false);
      }
    },

    async cancelInvite(repoId: number, invId: number): Promise<void> {
      modalBusy.set(true);
      modalError.set(null);
      try {
        await bridge.cancelInvitation(repoId, invId);
        membershipData.set(await bridge.listMembership(repoId));
      } catch (err: unknown) {
        modalError.set(handleError(err));
      } finally {
        modalBusy.set(false);
      }
    },

    async resendInvite(repoId: number, invId: number): Promise<void> {
      modalBusy.set(true);
      modalError.set(null);
      try {
        const res = await bridge.resendInvitation(repoId, invId);
        membershipData.set(await bridge.listMembership(repoId));
        toast(ctf("ch.toastInvited", { user: res.username, status: res.status }), "info");
      } catch (err: unknown) {
        modalError.set(handleError(err));
      } finally {
        modalBusy.set(false);
      }
    },

    async revokeMember(repoId: number, username: string): Promise<void> {
      modalBusy.set(true);
      modalError.set(null);
      try {
        const res = await bridge.revokeMember(repoId, username);
        membershipData.set(await bridge.listMembership(repoId));
        toast(ctf("ch.toastRevoked", { user: res.username, status: res.status }), "info");
        if (res.effective_role) {
          toast(
            ctf("ch.toastRevokeInheritedNote", {
              role: res.effective_role,
              source: res.access_source ?? "github",
            }),
            "warning",
          );
        }
      } catch (err: unknown) {
        modalError.set(handleError(err));
      } finally {
        modalBusy.set(false);
      }
    },

    async openRollbackModal(repoId: number, skillId: string): Promise<void> {
      batch(() => {
        rollbackTargets.set([]);
        rollbackRevision.set(null);
        rollbackDivergence.set("none");
        modalError.set(null);
        modal.set({ kind: "rollback", repositoryId: repoId, skillId });
        modalBusy.set(true);
      });
      try {
        const list = await bridge.listRollbackTargets(repoId, skillId);
        batch(() => {
          rollbackTargets.set(list);
          rollbackRevision.set(list[0]?.target.revision ?? null);
        });
      } catch (err: unknown) {
        modalError.set(handleError(err));
      } finally {
        modalBusy.set(false);
      }
    },

    async confirmRollback(): Promise<void> {
      const active = modal();
      if (active?.kind !== "rollback") return;
      const rev = rollbackRevision();
      const targetItem = rollbackTargets().find((t) => t.target.revision === rev);
      if (targetItem === undefined) return;
      const divMode = rollbackDivergence();
      const res: DivergenceChoice | null =
        divMode === "none"
          ? null
          : divMode === "discard"
            ? { type: "discard" }
            : { type: "preserve", local_name: `${active.skillId}.local` };
      modalBusy.set(true);
      modalError.set(null);
      try {
        const out = await bridge.rollbackSkill(active.repositoryId, active.skillId, targetItem.target, res);
        await closeModal();
        await refreshLocalSnapshot();
        notifyLocalSkillsChanged();
        toast(ctf("ch.toastRolledBack", { skill: active.skillId, tag: out.pin.target.tag_name }), "success");
      } catch (err: unknown) {
        modalError.set(handleError(err));
      } finally {
        modalBusy.set(false);
      }
    },

    openConvertLocalModal(repoId: number, skillId: string, suggestedName: string, revoked: boolean): void {
      batch(() => {
        convertLocalName.set(suggestedName || `${skillId}.local`);
        modalError.set(null);
        modal.set({ kind: "convert_local", repositoryId: repoId, skillId, revoked });
      });
    },

    async confirmConvertLocal(): Promise<void> {
      const active = modal();
      if (active?.kind !== "convert_local") return;
      const localName = convertLocalName().trim();
      if (localName === "") return;
      modalBusy.set(true);
      modalError.set(null);
      try {
        const res = await bridge.convertRemovedSkillToLocal(
          active.repositoryId,
          active.skillId,
          localName,
          active.revoked,
        );
        await closeModal();
        await refreshLocalSnapshot();
        notifyLocalSkillsChanged();
        toast(
          ctf("ch.toastConvertedLocal", {
            skill: active.skillId,
            local: res.local_name ?? localName,
          }),
          "success",
        );
      } catch (err: unknown) {
        modalError.set(handleError(err));
      } finally {
        modalBusy.set(false);
      }
    },

    openExportModal(repoId: number): void {
      batch(() => {
        exportOutDir.set("");
        modalError.set(null);
        modal.set({ kind: "export_marketplace", repositoryId: repoId });
      });
    },

    async browseExportDir(): Promise<void> {
      try {
        const picked = await bridge.pickExportFolder(ct("ch.exportOutLabel"));
        if (picked !== null && picked.trim() !== "") {
          exportOutDir.set(picked);
          modalError.set(null);
        }
      } catch {
        // Ignore dialog cancellation
      }
    },

    async confirmExport(): Promise<void> {
      const active = modal();
      if (active?.kind !== "export_marketplace") return;
      const dir = exportOutDir().trim();
      if (dir === "") return;
      modalBusy.set(true);
      modalError.set(null);
      try {
        const res = await bridge.exportMarketplace(active.repositoryId, dir);
        await closeModal();
        toast(ctf("ch.toastExported", { count: res.plugin_names.length, root: res.root }), "success");
      } catch (err: unknown) {
        modalError.set(handleError(err));
      } finally {
        modalBusy.set(false);
      }
    },
  };
}

export type ChannelsState = ReturnType<typeof createChannelsState>;

export const defaultChannelsState = createChannelsState(
  realChannelsBridge,
  realAuthControl,
  scheduleRefresh,
);
