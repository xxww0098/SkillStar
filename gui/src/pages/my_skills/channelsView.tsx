import { Scroll, Text, View, effect, untracked, type JSX } from "nsh";
import { errorAlert } from "../../chrome/alerts";
import { Icon } from "../../chrome/icons";
import { paletteToken } from "../../services/themeMode";
import { t } from "../../services/translation";
import { ChannelsModalHost } from "./channelsModals";
import { ChannelBtn, ChannelSkillsSection } from "./channelsSkillRows";
import {
  defaultChannelsState,
  type ChannelListEntry,
  type ChannelsState,
  type InboxInvitation,
} from "./channelsState";
import { ct, ctf } from "./channelsStrings";
import { scope } from "./state";

/**
 * 共享频道完整工作区（离线只读浏览 + 本地自动升级/导出 + GitHub 登录后全生命周期操作）。
 * 当外部未传入 `props.state` 时使用真实 `defaultChannelsState`，在首次进入 `channels` scope
 * 或 GitHub 登录状态变更为 `connected` 时自动同步本地/收件箱数据。
 */
export function ChannelsWorkspace(props: {
  state?: ChannelsState;
  autoStart?: boolean;
}): JSX.Element {
  const state = props.state ?? defaultChannelsState;
  const autoStart = props.autoStart !== false;

  if (autoStart) {
    effect(() => {
      const currentScope = scope();
      const authState = state.authCtrl.auth()?.state;
      if (currentScope === "channels") {
        untracked(() => {
          void state.refreshLocalSnapshot();
          if (authState === "connected") {
            void state.refreshInbox();
          }
        });
      }
    });
  } else {
    untracked(() => {
      void state.refreshAll();
    });
  }

  return (
    <View
      testID="channels-workspace"
      style={{
        position: "relative",
        display: "flex",
        flexDirection: "column",
        flex: 1,
        minHeight: 0,
        width: "100%",
        paddingHorizontal: 20,
        paddingVertical: 16,
        gap: 12,
      }}
    >
      <ChannelsTopBar state={state} />
      <If when={state.errorMessage() !== null}>
        {errorAlert(state.errorMessage() ?? "")}
      </If>
      <InboxBanner state={state} />
      <View
        style={{
          display: "flex",
          flexDirection: "row",
          flex: 1,
          minHeight: 0,
          gap: 14,
        }}
      >
        <ChannelsListPane state={state} />
        <ChannelDetailPane state={state} />
      </View>
      <ChannelsModalHost state={state} />
    </View>
  );
}

function ChannelsTopBar(props: { state: ChannelsState }): JSX.Element {
  const state = props.state;
  const isConnected = () => state.authCtrl.auth()?.state === "connected";
  const isExpired = () => state.authCtrl.auth()?.state === "expired";
  return (
    <View
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        justifyContent: "space-between",
        flexWrap: "wrap",
        gap: 10,
        padding: 12,
        borderRadius: 12,
        borderWidth: 1,
        borderColor: paletteToken("border"),
        backgroundColor: paletteToken("card"),
      }}
    >
      <View style={{ display: "flex", flexDirection: "column", gap: 3, flex: 1, minWidth: 220 }}>
        <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 8 }}>
          <Icon name="layers" size={16} color={paletteToken("violet")} />
          <Text style={{ fontSize: 15, fontWeight: "700", color: paletteToken("fg") }}>
            {t("emptyState.channelsTitle")}
          </Text>
          <View
            style={{
              paddingHorizontal: 8,
              paddingVertical: 2,
              borderRadius: 999,
              backgroundColor: paletteToken("accent_soft"),
            }}
          >
            <Text style={{ fontSize: 11, color: paletteToken("accent_fg") }}>
              {isConnected()
                ? ctf("ch.authConnected", { login: state.authCtrl.auth()?.login ?? "" })
                : ct("ch.offlineBadge")}
            </Text>
          </View>
        </View>
        <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>
          {isConnected()
            ? ct("ch.subtitle")
            : isExpired()
              ? ctf("ch.authExpired", { login: state.authCtrl.auth()?.login ?? "" })
              : ct("ch.authSignedOut")}
        </Text>
      </View>

      <View style={{ display: "flex", flexDirection: "row", alignItems: "center", flexWrap: "wrap", gap: 8 }}>
        <ChannelBtn
          testID="channels-refresh"
          icon="refresh"
          label={ct("ch.refreshBtn")}
          disabled={state.loading() || state.busyKey() !== null}
          onPress={() => void state.refreshAll()}
        />
        <If
          when={isConnected()}
          else={
            <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 8 }}>
              <If when={isExpired()}>
                <ChannelBtn
                  testID="channels-refresh-auth"
                  icon="refresh"
                  label={ct("ch.refreshAuthBtn")}
                  onPress={() => void state.refreshAuth()}
                />
              </If>
              <ChannelBtn
                testID="channels-github-sign-in"
                icon="gitBranch"
                label={t("sharedChannels.signInButton")}
                primary
                onPress={() => state.authCtrl.openSignIn()}
              />
            </View>
          }
        >
          <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 8 }}>
            <ChannelBtn
              testID="channels-create-btn"
              icon="plus"
              label={ct("ch.createBtn")}
              primary
              onPress={() => void state.openCreateModal()}
            />
            <ChannelBtn
              testID="channels-register-btn"
              icon="folderGit2"
              label={ct("ch.registerBtn")}
              onPress={() => void state.openRegisterModal()}
            />
            <ChannelBtn
              testID="channels-sign-out"
              label={ct("ch.signOutBtn")}
              onPress={() => void state.signOut()}
            />
          </View>
        </If>
      </View>
    </View>
  );
}

function InboxBanner(props: { state: ChannelsState }): JSX.Element {
  const state = props.state;
  return (
    <If when={state.authCtrl.auth()?.state === "connected" && state.inbox().length > 0}>
      {() => (
        <View
          testID="channels-inbox"
          style={{
            display: "flex",
            flexDirection: "column",
            gap: 8,
            padding: 12,
            borderRadius: 12,
            borderWidth: 1,
            borderColor: paletteToken("accent"),
            backgroundColor: paletteToken("accent_soft"),
          }}
        >
          <Text style={{ fontSize: 13, fontWeight: "600", color: paletteToken("fg") }}>
            {ctf("ch.inboxTitle", { count: state.inbox().length })}
          </Text>
          <Each of={state.inbox()} key={(inv: InboxInvitation) => String(inv.id)}>
            {(inv: () => InboxInvitation) => (
              <View
                testID={`channels-inbox-row-${inv().id}`}
                style={{
                  display: "flex",
                  flexDirection: "row",
                  alignItems: "center",
                  justifyContent: "space-between",
                  gap: 10,
                  padding: 8,
                  borderRadius: 8,
                  backgroundColor: paletteToken("card"),
                }}
              >
                <View style={{ display: "flex", flexDirection: "column", flex: 1, minWidth: 0 }}>
                  <Text style={{ fontSize: 12, fontWeight: "600", color: paletteToken("fg") }}>
                    {`${inv().owner}/${inv().repository_name}`}
                  </Text>
                  <Text style={{ fontSize: 11, color: paletteToken("fg_muted") }}>
                    {ctf("ch.inboxFrom", {
                      inviter: inv().inviter?.login ?? "github",
                      role: inv().effective_role,
                    })}
                  </Text>
                </View>
                <ChannelBtn
                  testID={`channels-inbox-accept-${inv().id}`}
                  label={ct("ch.inboxAccept")}
                  primary
                  disabled={state.busyKey() !== null}
                  onPress={() => void state.acceptInvitation(inv().id)}
                />
                <ChannelBtn
                  testID={`channels-inbox-decline-${inv().id}`}
                  label={ct("ch.inboxDecline")}
                  disabled={state.busyKey() !== null}
                  onPress={() => void state.declineInvitation(inv().id)}
                />
              </View>
            )}
          </Each>
        </View>
      )}
    </If>
  );
}

function ChannelsListPane(props: { state: ChannelsState }): JSX.Element {
  const state = props.state;
  return (
    <View
      testID="channels-list-pane"
      style={{
        display: "flex",
        flexDirection: "column",
        width: 260,
        flexShrink: 0,
        minHeight: 0,
        borderRadius: 12,
        borderWidth: 1,
        borderColor: paletteToken("border"),
        backgroundColor: paletteToken("card"),
        padding: 10,
        gap: 8,
      }}
    >
      <Text style={{ fontSize: 12, fontWeight: "600", color: paletteToken("fg_muted") }}>
        {ct("ch.listTitle")}
      </Text>
      <Scroll style={{ flex: 1, minHeight: 0 }} contentStyle={{ display: "flex", flexDirection: "column", gap: 6 }}>
        <If
          when={state.combinedEntries().length > 0}
          else={
            <View testID="channels-empty-local" style={{ paddingVertical: 16, gap: 6 }}>
              <Text style={{ fontSize: 13, fontWeight: "600", color: paletteToken("fg") }}>
                {ct("ch.emptyTitle")}
              </Text>
              <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>
                {ct("ch.emptyHint")}
              </Text>
            </View>
          }
        >
          <Each of={state.combinedEntries()} key={(e: ChannelListEntry) => String(e.repositoryId)}>
            {(entry: () => ChannelListEntry) => (
              <ChannelRailItem state={state} entry={entry()} />
            )}
          </Each>
        </If>
      </Scroll>
    </View>
  );
}

function ChannelRailItem(props: { state: ChannelsState; entry: ChannelListEntry }): JSX.Element {
  const state = props.state;
  const active = () => state.selectedEntry()?.repositoryId === props.entry.repositoryId;
  const isRevoked = () => props.entry.subscription?.view.remote_state.status === "revoked";
  const isPending = () =>
    props.entry.descriptor !== null && props.entry.descriptor.status !== "active";
  return (
    <View
      testID={`channels-item-${props.entry.repositoryId}`}
      style={{
        display: "flex",
        flexDirection: "column",
        gap: 4,
        padding: 10,
        borderRadius: 10,
        cursor: "pointer",
        borderWidth: 1,
        borderColor: active() ? paletteToken("accent") : paletteToken("border_soft"),
        backgroundColor: active() ? paletteToken("accent_soft") : paletteToken("panel"),
      }}
      onPress={() => state.selectChannel(props.entry.repositoryId)}
    >
      <Text style={{ fontSize: 13, fontWeight: "600", color: paletteToken("fg") }}>
        {props.entry.displayName}
      </Text>
      <View style={{ display: "flex", flexDirection: "row", flexWrap: "wrap", gap: 6 }}>
        <If when={props.entry.descriptor !== null}>
          <Text style={{ fontSize: 11, color: paletteToken("fg_muted") }}>
            {props.entry.descriptor?.role ?? ""}
          </Text>
        </If>
        <If when={isPending()}>
          <Text style={{ fontSize: 11, color: paletteToken("amber_fg") }}>
            {ct("ch.pendingBadge")}
          </Text>
        </If>
        <If when={isRevoked()}>
          <Text style={{ fontSize: 11, color: paletteToken("danger") }}>
            {ct("ch.revokedBadge")}
          </Text>
        </If>
        <If when={props.entry.subscription?.view.read_only === true}>
          <Text style={{ fontSize: 11, color: paletteToken("fg_muted") }}>
            {ct("ch.readOnlyBadge")}
          </Text>
        </If>
      </View>
    </View>
  );
}

function ChannelDetailPane(props: { state: ChannelsState }): JSX.Element {
  const state = props.state;
  const entry = () => state.selectedEntry();
  const isConnected = () => state.authCtrl.auth()?.state === "connected";
  const sub = () => entry()?.subscription ?? null;
  const desc = () => entry()?.descriptor ?? null;
  const isRevoked = () => sub()?.view.remote_state.status === "revoked";
  const isReadOnly = () => sub()?.view.read_only === true;
  const canPublish = () => desc()?.role === "owner" || desc()?.role === "publisher";
  const canManageMembers = () => desc()?.role === "owner";

  return (
    <View
      testID="channels-detail-pane"
      style={{
        display: "flex",
        flexDirection: "column",
        flex: 1,
        minWidth: 0,
        minHeight: 0,
        borderRadius: 12,
        borderWidth: 1,
        borderColor: paletteToken("border"),
        backgroundColor: paletteToken("card"),
        padding: 14,
      }}
    >
      <If
        when={entry() !== null}
        else={
          <View style={{ flex: 1, alignItems: "center", justifyContent: "center" }}>
            <Text style={{ fontSize: 13, color: paletteToken("fg_muted") }}>
              {ct("ch.selectHint")}
            </Text>
          </View>
        }
      >
        {() => (
          <Scroll style={{ flex: 1, minHeight: 0 }} contentStyle={{ display: "flex", flexDirection: "column", gap: 12 }}>
            <View style={{ display: "flex", flexDirection: "row", alignItems: "center", justifyContent: "space-between", flexWrap: "wrap", gap: 8 }}>
              <View style={{ display: "flex", flexDirection: "column", gap: 2 }}>
                <Text testID="channels-detail-title" style={{ fontSize: 16, fontWeight: "700", color: paletteToken("fg") }}>
                  {entry()?.displayName ?? ""}
                </Text>
                <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>
                  {sub()?.view.target
                    ? ctf("ch.releaseInfo", {
                        tag: sub()?.view.target?.tag_name ?? "",
                        count: sub()?.view.selected_skill_ids.length ?? 0,
                      })
                    : ct("ch.notSubscribedHint")}
                </Text>
              </View>

              <View style={{ display: "flex", flexDirection: "row", flexWrap: "wrap", gap: 8 }}>
                <If when={sub() !== null && !isReadOnly()}>
                  <ChannelBtn
                    testID="channels-auto-update-toggle"
                    icon="sliders"
                    label={`${ct("ch.autoUpdateLabel")}: ${sub()?.view.auto_update.enabled ? ct("ch.autoUpdateOn") : ct("ch.autoUpdateOff")}`}
                    disabled={state.busyKey() !== null}
                    onPress={() => {
                      const current = entry();
                      const currentSub = sub();
                      if (current && currentSub) {
                        void state.toggleAutoUpdate(
                          current.repositoryId,
                          !currentSub.view.auto_update.enabled,
                        );
                      }
                    }}
                  />
                </If>
                <If when={sub() !== null && !isReadOnly()}>
                  <ChannelBtn
                    testID="channels-export-btn"
                    icon="packageOpen"
                    label={ct("ch.exportBtn")}
                    onPress={() => {
                      const current = entry();
                      if (current) state.openExportModal(current.repositoryId);
                    }}
                  />
                </If>
              </View>
            </View>

            <If when={isReadOnly()}>
              <View style={{ padding: 10, borderRadius: 8, backgroundColor: paletteToken("panel") }}>
                <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>
                  {ct("ch.readOnlyNotice")}
                </Text>
              </View>
            </If>

            <If when={isRevoked()}>
              <View
                testID="channels-revoked-banner"
                style={{
                  padding: 10,
                  borderRadius: 8,
                  borderWidth: 1,
                  borderColor: paletteToken("danger_border"),
                  backgroundColor: paletteToken("danger_bg"),
                }}
              >
                <Text style={{ fontSize: 12, color: paletteToken("danger") }}>
                  {ct("ch.revokedNotice")}
                </Text>
              </View>
            </If>

            <If when={isConnected() && !isReadOnly()}>
              <View style={{ display: "flex", flexDirection: "row", flexWrap: "wrap", gap: 8 }}>
                <If when={desc()?.status === "awaiting_app_installation"}>
                  <ChannelBtn
                    testID="channels-resume-pending-btn"
                    icon="refresh"
                    label={ct("ch.resumeChannelBtn")}
                    primary
                    onPress={() => void state.resumePendingChannel(entry()!.repositoryId)}
                  />
                </If>
                <If when={desc()?.status === "awaiting_invitation_acceptance"}>
                  <ChannelBtn
                    testID="channels-resume-accepted-btn"
                    icon="refresh"
                    label={ct("ch.resumeAcceptedBtn")}
                    primary
                    onPress={() => void state.resumeAcceptedChannel(entry()!.repositoryId)}
                  />
                </If>
                <If when={(desc() === null || desc()!.status === "active") && sub() === null}>
                  <ChannelBtn
                    testID="channels-review-subscribe-btn"
                    icon="check"
                    label={ct("ch.reviewSubscribeBtn")}
                    primary
                    onPress={() => void state.openReviewModal(entry()!.repositoryId)}
                  />
                </If>
                <If when={sub() !== null && !isRevoked()}>
                  <ChannelBtn
                    testID="channels-check-update-btn"
                    icon="refresh"
                    label={ct("ch.checkUpdateBtn")}
                    disabled={state.busyKey() !== null}
                    onPress={() => void state.checkUpdates(entry()!.repositoryId)}
                  />
                </If>
                <If when={sub()?.update_snapshot !== null && !isRevoked()}>
                  <ChannelBtn
                    testID="channels-apply-update-btn"
                    icon="download"
                    label={ct("ch.applyUpdateBtn")}
                    primary
                    disabled={state.busyKey() !== null}
                    onPress={() =>
                      void state.applyUpdates(entry()!.repositoryId, sub()!.update_snapshot!)
                    }
                  />
                </If>
                <If when={desc()?.status === "active" && canPublish()}>
                  <ChannelBtn
                    testID="channels-publish-btn"
                    icon="externalLink"
                    label={ct("ch.publishBtn")}
                    onPress={() => void state.openPublishModal(entry()!.repositoryId)}
                  />
                </If>
                <If when={canManageMembers()}>
                  <ChannelBtn
                    testID="channels-members-btn"
                    icon="users"
                    label={ct("ch.membersBtn")}
                    onPress={() => void state.openMembersModal(entry()!.repositoryId)}
                  />
                </If>
              </View>
            </If>

            <ChannelSkillsSection state={state} entry={entry()!} />
          </Scroll>
        )}
      </If>
    </View>
  );
}
