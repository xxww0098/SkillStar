import { Text, View, type JSX } from "nsh";
import { Icon, type IconName } from "../../chrome/icons";
import { paletteToken } from "../../services/themeMode";
import type { ChannelListEntry, ChannelsState, ChannelUpdateItem } from "./channelsState";
import { ct, ctf } from "./channelsStrings";

function safeId(value: string): string {
  return value.replace(/[^A-Za-z0-9_-]/g, "-");
}

export function ChannelBtn(props: {
  testID: string;
  label: string;
  icon?: IconName;
  primary?: boolean;
  danger?: boolean;
  disabled?: boolean;
  onPress: () => void;
}): JSX.Element {
  return (
    <View
      testID={props.testID}
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        gap: 6,
        height: 30,
        paddingHorizontal: 10,
        borderRadius: 8,
        cursor: props.disabled ? "default" : "pointer",
        backgroundColor: props.primary
          ? paletteToken("accent")
          : props.danger
            ? paletteToken("danger_bg")
            : paletteToken("card"),
        borderWidth: props.primary ? 0 : 1,
        borderColor: props.danger ? paletteToken("danger_border") : paletteToken("border"),
        opacity: props.disabled ? 0.55 : 1,
      }}
      hoverStyle={
        props.disabled
          ? undefined
          : {
              backgroundColor: props.primary
                ? paletteToken("accent_hover")
                : paletteToken("panel_hover"),
            }
      }
      disabled={props.disabled ?? false}
      onPress={props.onPress}
    >
      <If when={props.icon !== undefined}>
        <Icon
          name={props.icon!}
          size={13}
          color={
            props.primary
              ? paletteToken("on_accent")
              : props.danger
                ? paletteToken("danger")
                : paletteToken("fg")
          }
        />
      </If>
      <Text
        style={{
          fontSize: 12,
          fontWeight: "500",
          color: props.primary
            ? paletteToken("on_accent")
            : props.danger
              ? paletteToken("danger")
              : paletteToken("fg"),
        }}
      >
        {props.label}
      </Text>
    </View>
  );
}

function itemStateLabel(itemState: ChannelUpdateItem["state"]): string {
  switch (itemState) {
    case "current":
      return ct("ch.statusCurrent");
    case "available":
      return ct("ch.statusAvailable");
    case "applied":
      return ct("ch.statusApplied");
    case "blocked":
      return ct("ch.statusBlocked");
    case "failed":
      return ct("ch.statusFailed");
    case "notification":
      return ct("ch.statusNotification");
    case "removed_from_channel":
      return ct("ch.statusRemoved");
  }
}

export function ChannelSkillsSection(props: {
  state: ChannelsState;
  entry: ChannelListEntry;
}): JSX.Element {
  const state = props.state;
  const sub = () => props.entry.subscription;
  const isConnected = () => state.authCtrl.auth()?.state === "connected";
  const isRevoked = () => sub()?.view.remote_state.status === "revoked";
  const isReadOnly = () => sub()?.view.read_only === true;
  const items = (): ChannelUpdateItem[] => {
    const s = sub();
    if (s === null) return [];
    if (s.update_snapshot !== null && s.update_snapshot.items.length > 0) {
      return s.update_snapshot.items;
    }
    return s.view.selected_skill_ids.map((id) => ({
      id,
      change: "unchanged",
      state: "current",
      selected: true,
      from_content_hash: null,
      to_content_hash: null,
      block_reason: null,
      suggested_local_name: null,
      error: null,
    }));
  };
  const pinFor = (skillId: string) =>
    sub()?.pins.find((p) => p.skill_id.toLowerCase() === skillId.toLowerCase()) ?? null;

  return (
    <If when={sub() !== null}>
      {() => (
        <View style={{ display: "flex", flexDirection: "column", gap: 8 }}>
          <Text style={{ fontSize: 13, fontWeight: "600", color: paletteToken("fg") }}>
            {ct("ch.skillsSectionTitle")}
          </Text>
          <Each of={items()} key={(it: ChannelUpdateItem) => it.id}>
            {(item: () => ChannelUpdateItem) => {
              const sid = () => safeId(item().id);
              const pinned = () => item().pinned_target ?? pinFor(item().id)?.target ?? null;
              const removedOrRevoked = () =>
                item().state === "removed_from_channel" || isRevoked();
              const chosenRes = () => state.getResolution(props.entry.repositoryId, item().id);
              return (
                <View
                  testID={`channels-skill-row-${sid()}`}
                  style={{
                    display: "flex",
                    flexDirection: "column",
                    gap: 8,
                    padding: 10,
                    borderRadius: 10,
                    borderWidth: 1,
                    borderColor: paletteToken("border_soft"),
                    backgroundColor: paletteToken("panel"),
                  }}
                >
                  <View style={{ display: "flex", flexDirection: "row", alignItems: "center", justifyContent: "space-between", flexWrap: "wrap", gap: 8 }}>
                    <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 8 }}>
                      <Text style={{ fontSize: 13, fontWeight: "600", color: paletteToken("fg") }}>
                        {item().id}
                      </Text>
                      <Text style={{ fontSize: 11, color: paletteToken("fg_muted") }}>
                        {itemStateLabel(item().state)}
                      </Text>
                      <If when={pinned() !== null}>
                        <Text
                          testID={`channels-pin-badge-${sid()}`}
                          style={{ fontSize: 11, color: paletteToken("accent_fg") }}
                        >
                          {ctf("ch.pinnedBadge", { tag: pinned()?.tag_name ?? "" })}
                        </Text>
                      </If>
                    </View>

                    <If when={!isReadOnly()}>
                      <View style={{ display: "flex", flexDirection: "row", flexWrap: "wrap", gap: 6 }}>
                        <If when={isConnected() && item().state === "notification" && !isRevoked()}>
                          <ChannelBtn
                            testID={`channels-install-${sid()}`}
                            icon="plus"
                            label={ct("ch.installSkillBtn")}
                            primary
                            onPress={() =>
                              void state.installChannelSkill(props.entry.repositoryId, item().id)
                            }
                          />
                        </If>
                        <If when={isConnected() && pinned() !== null && !isRevoked()}>
                          <ChannelBtn
                            testID={`channels-unpin-${sid()}`}
                            icon="check"
                            label={ct("ch.unpinBtn")}
                            onPress={() =>
                              void state.resumeFollowingSkill(props.entry.repositoryId, item().id)
                            }
                          />
                        </If>
                        <If when={isConnected() && item().selected && !removedOrRevoked()}>
                          <ChannelBtn
                            testID={`channels-rollback-${sid()}`}
                            icon="rotateCcw"
                            label={ct("ch.rollbackBtn")}
                            onPress={() =>
                              void state.openRollbackModal(props.entry.repositoryId, item().id)
                            }
                          />
                        </If>
                        <If when={isConnected() && removedOrRevoked()}>
                          <ChannelBtn
                            testID={`channels-removed-to-local-${sid()}`}
                            icon="copy"
                            label={ct("ch.removedToLocalBtn")}
                            onPress={() =>
                              state.openConvertLocalModal(
                                props.entry.repositoryId,
                                item().id,
                                item().suggested_local_name ?? `${item().id}.local`,
                                isRevoked(),
                              )
                            }
                          />
                          <ChannelBtn
                            testID={`channels-removed-uninstall-${sid()}`}
                            icon="trash"
                            label={ct("ch.removedUninstallBtn")}
                            danger
                            onPress={() =>
                              void state.uninstallRemovedSkill(
                                props.entry.repositoryId,
                                item().id,
                                isRevoked(),
                              )
                            }
                          />
                        </If>
                      </View>
                    </If>
                  </View>

                  <If when={item().error !== null}>
                    <Text style={{ fontSize: 11, color: paletteToken("danger") }}>
                      {item().error}
                    </Text>
                  </If>

                  <If when={item().state === "blocked" && isConnected() && !isReadOnly()}>
                    <View
                      testID={`channels-conflict-${sid()}`}
                      style={{
                        display: "flex",
                        flexDirection: "column",
                        gap: 6,
                        padding: 8,
                        borderRadius: 8,
                        borderWidth: 1,
                        borderColor: paletteToken("amber_border"),
                        backgroundColor: paletteToken("amber_bg"),
                      }}
                    >
                      <Text style={{ fontSize: 11, color: paletteToken("fg_muted") }}>
                        {ct("ch.conflictNeedsChoice")}
                      </Text>
                      <View style={{ display: "flex", flexDirection: "row", flexWrap: "wrap", gap: 6 }}>
                        <ChannelBtn
                          testID={`channels-conflict-preserve-${sid()}`}
                          label={ctf("ch.conflictPreserve", {
                            name: item().suggested_local_name ?? `${item().id}.local`,
                          })}
                          primary={chosenRes()?.type === "preserve"}
                          onPress={() =>
                            state.setResolution(props.entry.repositoryId, item().id, {
                              type: "preserve",
                              local_name: item().suggested_local_name ?? `${item().id}.local`,
                            })
                          }
                        />
                        <ChannelBtn
                          testID={`channels-conflict-discard-${sid()}`}
                          label={ct("ch.conflictDiscard")}
                          primary={chosenRes()?.type === "discard"}
                          danger={chosenRes()?.type !== "discard"}
                          onPress={() =>
                            state.setResolution(props.entry.repositoryId, item().id, {
                              type: "discard",
                            })
                          }
                        />
                      </View>
                    </View>
                  </If>
                </View>
              );
            }}
          </Each>
        </View>
      )}
    </If>
  );
}
