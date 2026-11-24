import { Button } from "nsh/base";
import { Text, View, mount, signal, type JSX } from "nsh";
import { ChannelsWorkspace } from "./channelsView";
import {
  createChannelsState,
  type ChannelsAuthControl,
  type ChannelsBridge,
  type ChannelUpdateSnapshot,
  type LocalChannelsSnapshot,
} from "./channelsState";

const authSig = signal<{ state: string; login: string | null } | null>({
  state: "signed_out",
  login: null,
});

const opsLog = signal<string[]>([]);
const logOp = (msg: string) => opsLog.set([...opsLog(), msg]);

let channelsStore: LocalChannelsSnapshot = {
  channels: [
    {
      descriptor_version: 1,
      repository_id: 101,
      organization_id: 10,
      owner: "acme",
      name: "team-skills",
      html_url: "https://github.com/acme/team-skills",
      clone_url: "https://github.com/acme/team-skills.git",
      role: "owner",
      status: "active",
      created_at: "2026-11-01T00:00:00Z",
      updated_at: "2026-11-01T00:00:00Z",
    },
  ],
  subscriptions: [
    {
      view: {
        schema_version: 1,
        descriptor_version: 4,
        repository_id: 101,
        organization_id: 10,
        target: { revision: 1, tag_name: "channel-v000001", commit_sha: "abc123" },
        selected_skill_ids: ["alpha", "beta"],
        auto_update: { enabled: true },
        remote_state: { status: "active" },
        read_only: false,
      },
      update_snapshot: {
        target: { revision: 2, tag_name: "channel-v000002", commit_sha: "def456" },
        title: "v2",
        notes: "",
        publisher: { id: 1, login: "octocat" },
        published_at: "2026-11-02T00:00:00Z",
        checked_at: "2026-11-02T00:00:00Z",
        status: "blocked",
        acknowledgement_required: false,
        items: [
          {
            id: "alpha",
            change: "updated",
            state: "blocked",
            selected: true,
            from_content_hash: "h1",
            to_content_hash: "h2",
            block_reason: "local_content_changed",
            suggested_local_name: "alpha.local",
            error: null,
            pinned_target: null,
          },
          {
            id: "beta",
            change: "removed",
            state: "removed_from_channel",
            selected: true,
            from_content_hash: "h1",
            to_content_hash: null,
            block_reason: "removed_upstream",
            suggested_local_name: "beta.local",
            error: null,
            pinned_target: { revision: 1, tag_name: "channel-v000001", commit_sha: "abc123" },
          },
          {
            id: "gamma",
            change: "added",
            state: "notification",
            selected: false,
            from_content_hash: null,
            to_content_hash: "h3",
            block_reason: null,
            suggested_local_name: null,
            error: null,
            pinned_target: null,
          },
          {
            id: "delta",
            change: "removed",
            state: "removed_from_channel",
            selected: true,
            from_content_hash: "h1",
            to_content_hash: null,
            block_reason: "removed_upstream",
            suggested_local_name: "delta.local",
            error: null,
            pinned_target: null,
          },
        ],
      },
      pins: [
        {
          skill_id: "beta",
          target: { revision: 1, tag_name: "channel-v000001", commit_sha: "abc123" },
        },
      ],
    },
  ],
};

let inboxStore = [
  {
    id: 901,
    repository_id: 202,
    owner: "acme",
    repository_name: "sec-skills",
    inviter: { id: 7, login: "maintainer" },
    effective_role: "subscriber" as const,
  },
];

const fakeBridge: ChannelsBridge = {
  async snapshot() {
    return JSON.parse(JSON.stringify(channelsStore)) as LocalChannelsSnapshot;
  },
  async setAutoUpdate(repoId, enabled) {
    logOp(`autoUpdate:${repoId}:${enabled}`);
    const sub = channelsStore.subscriptions.find((s) => s.view.repository_id === repoId);
    if (sub) sub.view.auto_update.enabled = enabled;
    return { enabled };
  },
  async exportMarketplace(repoId, outDir) {
    logOp(`export:${repoId}:${outDir}`);
    return {
      root: outDir,
      repository_id: repoId,
      revision: 1,
      plugin_names: ["alpha"],
      files: 3,
    };
  },
  async pickExportFolder() {
    return "/tmp/exported-mkt";
  },
  async listOrganizations() {
    return [{ id: 10, login: "acme", viewer_is_admin: true }];
  },
  async createChannel(org, name) {
    logOp(`create:${org}/${name}`);
    const desc = {
      descriptor_version: 1,
      repository_id: 303,
      organization_id: 10,
      owner: org,
      name,
      html_url: `https://github.com/${org}/${name}`,
      clone_url: `https://github.com/${org}/${name}.git`,
      role: "owner" as const,
      status: "active" as const,
      created_at: "2026-11-20T00:00:00Z",
      updated_at: "2026-11-20T00:00:00Z",
    };
    channelsStore.channels.push(desc);
    return desc;
  },
  async resumeChannel(repoId) {
    return channelsStore.channels.find((c) => c.repository_id === repoId)!;
  },
  async listRegistrationCandidates() {
    return [
      {
        repository_id: 404,
        owner: "acme",
        name: "legacy-repo",
        role: "owner",
        already_registered: false,
      },
    ];
  },
  async scanExistingRepo(orgId, repoId, sessionId) {
    logOp(`scan:${orgId}:${repoId}`);
    return {
      session_id: sessionId,
      repository: { repository_id: repoId, organization_id: orgId, owner: "acme", name: "legacy-repo" },
      total_files: 5,
      unclassified_files: ["README.md"],
      skills: [{ id: "legacy-skill", relative_dir: "legacy-skill" }],
    };
  },
  async confirmRegistration(sessionId) {
    logOp(`confirmReg:${sessionId}`);
    const desc = {
      descriptor_version: 1,
      repository_id: 404,
      organization_id: 10,
      owner: "acme",
      name: "legacy-repo",
      html_url: "https://github.com/acme/legacy-repo",
      clone_url: "https://github.com/acme/legacy-repo.git",
      role: "owner" as const,
      status: "active" as const,
      created_at: "2026-11-20T00:00:00Z",
      updated_at: "2026-11-20T00:00:00Z",
    };
    channelsStore.channels.push(desc);
    return desc;
  },
  async cancelRegistration() {
    return true;
  },
  async publishPreview(repoId, sessionId) {
    return {
      session_id: sessionId,
      repository_id: repoId,
      tag_name: "channel-v000003",
      commit_sha: "1234567890abcdef",
      skills: [{ id: "alpha", status: "included" }],
    };
  },
  async publishRelease(sessionId, title) {
    logOp(`publish:${sessionId}:${title}`);
    return { manifest: { tag_name: "channel-v000003" } };
  },
  async cancelPublish() {
    return true;
  },
  async listMembership() {
    return {
      members: [{ user: { id: 1, login: "octocat" }, role: "owner", github_role_name: "admin" }],
      invitations: [],
    };
  },
  async inviteMember(repoId, username, role) {
    logOp(`invite:${repoId}:${username}:${role}`);
    return { username, status: "invited" };
  },
  async cancelInvitation() {
    return { username: "bob" };
  },
  async resendInvitation() {
    return { username: "bob", status: "resent" };
  },
  async revokeMember(repoId, username) {
    logOp(`revoke:${repoId}:${username}`);
    return { username, status: "revoked" };
  },
  async listInbox() {
    return [...inboxStore];
  },
  async acceptInvitation(invId) {
    logOp(`accept:${invId}`);
    inboxStore = inboxStore.filter((i) => i.id !== invId);
    const desc = {
      descriptor_version: 1,
      repository_id: 202,
      organization_id: 10,
      owner: "acme",
      name: "sec-skills",
      html_url: "https://github.com/acme/sec-skills",
      clone_url: "https://github.com/acme/sec-skills.git",
      role: "subscriber" as const,
      status: "active" as const,
      created_at: "2026-11-20T00:00:00Z",
      updated_at: "2026-11-20T00:00:00Z",
    };
    channelsStore.channels.push(desc);
    return desc;
  },
  async resumeAcceptedChannel(repoId) {
    return channelsStore.channels.find((c) => c.repository_id === repoId)!;
  },
  async declineInvitation(invId) {
    inboxStore = inboxStore.filter((i) => i.id !== invId);
    return { id: invId };
  },
  async reviewChannel(repoId) {
    return {
      repository_id: repoId,
      target: { revision: 1, tag_name: "channel-v000001", commit_sha: "abc" },
      title: "Initial",
      notes: "",
      publisher: { id: 1, login: "maintainer" },
      skills: [{ id: "sec-check", status: "active" }],
    };
  },
  async subscribeChannel(repoId, target, ids) {
    logOp(`subscribe:${repoId}:${ids.join(",")}`);
    channelsStore.subscriptions.push({
      view: {
        schema_version: 1,
        descriptor_version: 4,
        repository_id: repoId,
        organization_id: 10,
        target,
        selected_skill_ids: ids,
        auto_update: { enabled: true },
        remote_state: { status: "active" },
        read_only: false,
      },
      update_snapshot: null,
      pins: [],
    });
    return { target };
  },
  async checkUpdate(repoId) {
    logOp(`check:${repoId}`);
    return channelsStore.subscriptions[0].update_snapshot as ChannelUpdateSnapshot;
  },
  async applyUpdate(repoId, _target, res) {
    logOp(`apply:${repoId}:${res.map((r) => `${r.skill_id}=${r.resolution.type}`).join(",")}`);
    const snap = channelsStore.subscriptions[0].update_snapshot!;
    snap.status = "up_to_date";
    snap.items[0].state = "applied";
    return { snapshot: snap, applied_skill_ids: ["alpha"] };
  },
  async listRollbackTargets() {
    return [
      {
        target: { revision: 1, tag_name: "channel-v000001", commit_sha: "abc" },
        title: "v1",
        published_at: "2026-11-01",
      },
    ];
  },
  async rollbackSkill(repoId, skillId, target) {
    logOp(`rollback:${repoId}:${skillId}:${target.tag_name}`);
    const pin = { skill_id: skillId, target };
    channelsStore.subscriptions[0].pins.push(pin);
    return { pin, snapshot: channelsStore.subscriptions[0].update_snapshot! };
  },
  async resumeFollowingSkill(repoId, skillId) {
    logOp(`unpin:${repoId}:${skillId}`);
    channelsStore.subscriptions[0].pins = channelsStore.subscriptions[0].pins.filter(
      (p) => p.skill_id !== skillId,
    );
    const snap = channelsStore.subscriptions[0].update_snapshot!;
    for (const it of snap.items) {
      if (it.id === skillId) it.pinned_target = null;
    }
    return snap;
  },
  async installChannelSkill(repoId, skillId) {
    logOp(`install:${repoId}:${skillId}`);
    const snap = channelsStore.subscriptions[0].update_snapshot!;
    for (const it of snap.items) {
      if (it.id === skillId) {
        it.state = "current";
        it.selected = true;
      }
    }
    return { snapshot: snap };
  },
  async uninstallRemovedSkill(repoId, skillId) {
    logOp(`uninstallRemoved:${repoId}:${skillId}`);
    const snap = channelsStore.subscriptions[0].update_snapshot!;
    snap.items = snap.items.filter((i) => i.id !== skillId);
    return { skill_id: skillId };
  },
  async convertRemovedSkillToLocal(repoId, skillId, localName) {
    logOp(`convertLocal:${repoId}:${skillId}:${localName}`);
    const snap = channelsStore.subscriptions[0].update_snapshot!;
    snap.items = snap.items.filter((i) => i.id !== skillId);
    return { skill_id: skillId, local_name: localName };
  },
};

const fakeAuth: ChannelsAuthControl = {
  auth: authSig,
  openSignIn() {
    authSig.set({ state: "connected", login: "octocat" });
  },
  async logout() {
    authSig.set({ state: "signed_out", login: null });
  },
  async refreshAuth() {
    authSig.set({ state: "connected", login: "octocat" });
  },
};

const state = createChannelsState(fakeBridge, fakeAuth, () => logOp("localRefresh"));

function App(): JSX.Element {
  return (
    <View style={{ display: "flex", flexDirection: "column", width: "100%", height: "100%" }}>
      <View style={{ display: "flex", flexDirection: "row", gap: 8, padding: 6 }}>
        <Button
          testID="host-sign-in"
          onPress={() => {
            authSig.set({ state: "connected", login: "octocat" });
            void state.refreshAll();
          }}
        >
          <Text>sign-in</Text>
        </Button>
        <Text testID="host-ops-log">
          {() => `ops:${opsLog().join(";")} dir:${state.exportOutDir()}`}
        </Text>
      </View>
      <View style={{ display: "flex", flex: 1, minHeight: 0 }}>
        <ChannelsWorkspace state={state} autoStart={false} />
      </View>
    </View>
  );
}

export default App;
mount(App);
