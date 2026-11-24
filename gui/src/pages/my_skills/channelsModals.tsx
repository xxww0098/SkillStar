import { Overlay, Scroll, Text, View, type JSX } from "nsh";
import { Input } from "nsh/base";
import { errorAlert } from "../../chrome/alerts";
import { promptVeil, SHEET_SHADOW } from "../../chrome/dialog";
import { Icon } from "../../chrome/icons";
import { fadeTransition } from "../../chrome/motion";
import { isLight, paletteToken } from "../../services/themeMode";
import type {
  ActiveModal,
  ChannelInvitationRecord,
  ChannelMemberRecord,
  ChannelsState,
  GitHubOrgSummary,
  RegistrationCandidateRepo,
  RollbackTargetItem,
} from "./channelsState";
import { ct, ctf } from "./channelsStrings";

export function ChannelsModalHost(props: { state: ChannelsState }): JSX.Element {
  const state = props.state;
  return (
    <If when={state.modal() !== null}>
      {() => (
        <Overlay
          style={{ position: "fixed", top: 0, left: 0, right: 0, bottom: 0, zIndex: 60 }}
          onCancel={() => void state.closeModal()}
        >
          <View
            testID="channels-modal-veil"
            style={{
              position: "absolute",
              top: 0,
              left: 0,
              right: 0,
              bottom: 0,
              backgroundColor: promptVeil(),
            }}
            onPress={() => void state.closeModal()}
          />
          <View
            style={{
              position: "absolute",
              top: 0,
              left: 0,
              right: 0,
              bottom: 0,
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              padding: 20,
            }}
          >
            <View
              testID="channels-modal-card"
              style={{
                display: "flex",
                flexDirection: "column",
                width: 520,
                maxWidth: "100%",
                maxHeight: "90%",
                padding: 18,
                gap: 12,
                borderRadius: 12,
                borderWidth: 1,
                borderColor: paletteToken("border"),
                backgroundColor: isLight() ? paletteToken("card") : paletteToken("panel"),
                boxShadow: SHEET_SHADOW,
                ...fadeTransition(true),
              }}
              onPress={() => {}}
            >
              <ModalHeader state={state} />
              <If when={state.modalError() !== null}>
                {errorAlert(state.modalError() ?? "")}
              </If>
              <Scroll style={{ width: "100%", maxHeight: 420 }} contentStyle={{ display: "flex", flexDirection: "column", gap: 10 }}>
                <If when={state.modal()?.kind === "create"}>
                  <CreateChannelBody state={state} />
                </If>
                <If when={state.modal()?.kind === "register"}>
                  <RegisterExistingBody state={state} />
                </If>
                <If when={state.modal()?.kind === "review"}>
                  <ReviewSubscribeBody state={state} />
                </If>
                <If when={state.modal()?.kind === "publish"}>
                  <PublishReleaseBody state={state} />
                </If>
                <If when={state.modal()?.kind === "members"}>
                  <MembersModalBody state={state} />
                </If>
                <If when={state.modal()?.kind === "rollback"}>
                  <RollbackModalBody state={state} />
                </If>
                <If when={state.modal()?.kind === "convert_local"}>
                  <ConvertLocalModalBody state={state} />
                </If>
                <If when={state.modal()?.kind === "export_marketplace"}>
                  <ExportMarketplaceBody state={state} />
                </If>
              </Scroll>
            </View>
          </View>
        </Overlay>
      )}
    </If>
  );
}

function modalTitle(active: ActiveModal): string {
  if (active === null) return "";
  switch (active.kind) {
    case "create":
      return ct("ch.createModalTitle");
    case "register":
      return ct("ch.registerModalTitle");
    case "review":
      return ct("ch.reviewModalTitle");
    case "publish":
      return ct("ch.publishModalTitle");
    case "members":
      return ct("ch.membersModalTitle");
    case "rollback":
      return ctf("ch.rollbackModalTitle", { skill: active.skillId });
    case "convert_local":
      return ct("ch.convertModalTitle");
    case "export_marketplace":
      return ct("ch.exportModalTitle");
  }
}

function ModalHeader(props: { state: ChannelsState }): JSX.Element {
  const state = props.state;
  return (
    <View style={{ display: "flex", flexDirection: "row", alignItems: "center", justifyContent: "space-between", gap: 12 }}>
      <Text style={{ fontSize: 15, fontWeight: "700", color: paletteToken("fg") }}>
        {modalTitle(state.modal())}
      </Text>
      <View
        testID="channels-modal-close"
        style={{ padding: 6, borderRadius: 6, cursor: "pointer" }}
        hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
        onPress={() => void state.closeModal()}
      >
        <Icon name="x" size={16} color={paletteToken("fg_muted")} />
      </View>
    </View>
  );
}

function inputStyle() {
  return {
    width: "100%" as const,
    fontSize: 12,
    color: paletteToken("fg"),
    padding: 8,
    backgroundColor: paletteToken("input"),
    borderWidth: 1,
    borderColor: paletteToken("border"),
    borderRadius: 8,
  };
}

function ActionButton(props: {
  testID: string;
  label: string;
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
        justifyContent: "center",
        gap: 6,
        height: 30,
        paddingHorizontal: 12,
        borderRadius: 8,
        cursor: props.disabled ? "default" : "pointer",
        backgroundColor: props.primary
          ? paletteToken("accent")
          : props.danger
            ? paletteToken("danger_bg")
            : paletteToken("card"),
        borderWidth: props.primary ? 0 : 1,
        borderColor: props.danger ? paletteToken("danger_border") : paletteToken("border"),
        opacity: props.disabled ? 0.5 : 1,
      }}
      disabled={props.disabled ?? false}
      onPress={props.onPress}
    >
      <Text
        style={{
          fontSize: 12,
          fontWeight: "600",
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

function CreateChannelBody(props: { state: ChannelsState }): JSX.Element {
  const state = props.state;
  return (
    <View style={{ display: "flex", flexDirection: "column", gap: 10 }}>
      <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>{ct("ch.orgLabel")}</Text>
      <If when={state.orgs().length === 0} else={
        <View style={{ display: "flex", flexDirection: "row", flexWrap: "wrap", gap: 6 }}>
          <Each of={state.orgs()} key={(org: GitHubOrgSummary) => String(org.id)}>
            {(org: () => GitHubOrgSummary) => (
              <View
                testID={`channels-create-org-${org().login}`}
                style={{
                  paddingHorizontal: 10,
                  paddingVertical: 5,
                  borderRadius: 8,
                  cursor: "pointer",
                  borderWidth: 1,
                  borderColor: state.createOrg() === org().login ? paletteToken("accent") : paletteToken("border"),
                  backgroundColor: state.createOrg() === org().login ? paletteToken("accent_soft") : paletteToken("card"),
                }}
                onPress={() => state.createOrg.set(org().login)}
              >
                <Text style={{ fontSize: 12, color: paletteToken("fg") }}>{org().login}</Text>
              </View>
            )}
          </Each>
        </View>
      }>
        <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>{ct("ch.noOrgs")}</Text>
      </If>

      <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>{ct("ch.repoNameLabel")}</Text>
      <Input
        testID="channels-create-name-input"
        value={state.createName()}
        placeholder={ct("ch.repoNamePlaceholder")}
        disabled={state.modalBusy()}
        onChange={(e: { value: string }) => state.createName.set(e.value)}
        style={inputStyle()}
      />

      <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>{ct("ch.repoDescLabel")}</Text>
      <Input
        testID="channels-create-desc-input"
        value={state.createDesc()}
        placeholder={ct("ch.repoDescPlaceholder")}
        disabled={state.modalBusy()}
        onChange={(e: { value: string }) => state.createDesc.set(e.value)}
        style={inputStyle()}
      />

      <View style={{ display: "flex", flexDirection: "row", justifyContent: "flex-end", gap: 8 }}>
        <ActionButton testID="channels-create-cancel" label={ct("ch.modalCancel")} onPress={() => void state.closeModal()} />
        <ActionButton
          testID="channels-create-submit"
          label={ct("ch.createSubmit")}
          primary
          disabled={state.modalBusy() || state.createOrg().trim() === "" || state.createName().trim() === ""}
          onPress={() => void state.submitCreate()}
        />
      </View>
    </View>
  );
}

function RegisterExistingBody(props: { state: ChannelsState }): JSX.Element {
  const state = props.state;
  return (
    <View style={{ display: "flex", flexDirection: "column", gap: 10 }}>
      <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>{ct("ch.orgLabel")}</Text>
      <View style={{ display: "flex", flexDirection: "row", flexWrap: "wrap", gap: 6 }}>
        <Each of={state.orgs()} key={(org: GitHubOrgSummary) => String(org.id)}>
          {(org: () => GitHubOrgSummary) => (
            <View
              testID={`channels-register-org-${org().id}`}
              style={{
                paddingHorizontal: 10,
                paddingVertical: 5,
                borderRadius: 8,
                cursor: "pointer",
                borderWidth: 1,
                borderColor: state.registerOrgId() === org().id ? paletteToken("accent") : paletteToken("border"),
                backgroundColor: state.registerOrgId() === org().id ? paletteToken("accent_soft") : paletteToken("card"),
              }}
              onPress={() => void state.selectRegisterOrg(org().id)}
            >
              <Text style={{ fontSize: 12, color: paletteToken("fg") }}>{org().login}</Text>
            </View>
          )}
        </Each>
      </View>

      <Each of={state.registerCandidates()} key={(c: RegistrationCandidateRepo) => String(c.repository_id)}>
        {(cand: () => RegistrationCandidateRepo) => (
          <View
            testID={`channels-register-cand-${cand().repository_id}`}
            style={{
              display: "flex",
              flexDirection: "row",
              alignItems: "center",
              justifyContent: "space-between",
              gap: 8,
              padding: 8,
              borderRadius: 8,
              borderWidth: 1,
              borderColor: paletteToken("border_soft"),
              backgroundColor: paletteToken("card"),
            }}
          >
            <Text style={{ fontSize: 12, color: paletteToken("fg"), flex: 1 }}>
              {`${cand().owner}/${cand().name} (${cand().role})`}
            </Text>
            <ActionButton
              testID={`channels-register-scan-${cand().repository_id}`}
              label={ct("ch.scanRepoBtn")}
              disabled={state.modalBusy() || cand().already_registered || state.registerOrgId() === null}
              onPress={() => {
                const orgId = state.registerOrgId();
                if (orgId !== null) void state.scanCandidate(orgId, cand().repository_id);
              }}
            />
          </View>
        )}
      </Each>

      <If when={state.registerPreview() !== null}>
        {() => (
          <View
            testID="channels-register-preview"
            style={{
              display: "flex",
              flexDirection: "column",
              gap: 8,
              padding: 10,
              borderRadius: 8,
              borderWidth: 1,
              borderColor: paletteToken("accent"),
              backgroundColor: paletteToken("accent_soft"),
            }}
          >
            <Text style={{ fontSize: 12, color: paletteToken("fg") }}>
              {ctf("ch.exposureWarning", {
                total: state.registerPreview()?.total_files ?? 0,
                skills: state.registerPreview()?.skills.length ?? 0,
                unclassified: state.registerPreview()?.unclassified_files.length ?? 0,
              })}
            </Text>
            <View style={{ display: "flex", flexDirection: "row", justifyContent: "flex-end", gap: 8 }}>
              <ActionButton testID="channels-register-cancel" label={ct("ch.modalCancel")} onPress={() => void state.closeModal()} />
              <ActionButton
                testID="channels-register-confirm"
                label={ct("ch.confirmRegisterBtn")}
                primary
                disabled={state.modalBusy()}
                onPress={() => void state.confirmRegister()}
              />
            </View>
          </View>
        )}
      </If>
    </View>
  );
}

function ReviewSubscribeBody(props: { state: ChannelsState }): JSX.Element {
  const state = props.state;
  const selectedCount = () => Object.values(state.reviewSelected()).filter(Boolean).length;
  return (
    <If when={state.reviewData() !== null}>
      {() => (
        <View style={{ display: "flex", flexDirection: "column", gap: 10 }}>
          <Text testID="channels-review-header" style={{ fontSize: 12, fontWeight: "600", color: paletteToken("fg") }}>
            {ctf("ch.reviewHeader", {
              tag: state.reviewData()?.target.tag_name ?? "",
              title: state.reviewData()?.title ?? "",
              publisher: state.reviewData()?.publisher.login ?? "",
            })}
          </Text>
          <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>{ct("ch.reviewSkillsHint")}</Text>
          <Each
            of={(state.reviewData()?.skills ?? []).filter((s) => s.status !== "removed")}
            key={(s: { id: string; status: string }) => s.id}
          >
            {(sk: () => { id: string; status: string }) => (
              <View
                testID={`channels-review-skill-${sk().id.replace(/[^A-Za-z0-9_-]/g, "-")}`}
                style={{
                  display: "flex",
                  flexDirection: "row",
                  alignItems: "center",
                  justifyContent: "space-between",
                  padding: 8,
                  borderRadius: 8,
                  cursor: "pointer",
                  borderWidth: 1,
                  borderColor:
                    state.reviewSelected()[sk().id] === true ? paletteToken("accent") : paletteToken("border"),
                  backgroundColor: paletteToken("card"),
                }}
                onPress={() => state.toggleReviewSkill(sk().id)}
              >
                <Text style={{ fontSize: 12, color: paletteToken("fg") }}>{sk().id}</Text>
                <Icon
                  name={state.reviewSelected()[sk().id] === true ? "squareCheck" : "square"}
                  size={15}
                  color={state.reviewSelected()[sk().id] === true ? paletteToken("accent_fg") : paletteToken("fg_muted")}
                />
              </View>
            )}
          </Each>
          <View style={{ display: "flex", flexDirection: "row", justifyContent: "flex-end", gap: 8 }}>
            <ActionButton testID="channels-review-cancel" label={ct("ch.modalCancel")} onPress={() => void state.closeModal()} />
            <ActionButton
              testID="channels-review-confirm"
              label={ctf("ch.confirmSubscribeBtn", { count: selectedCount() })}
              primary
              disabled={state.modalBusy()}
              onPress={() => void state.confirmSubscribe()}
            />
          </View>
        </View>
      )}
    </If>
  );
}

function PublishReleaseBody(props: { state: ChannelsState }): JSX.Element {
  const state = props.state;
  return (
    <If when={state.publishPreviewData() !== null}>
      {() => (
        <View style={{ display: "flex", flexDirection: "column", gap: 10 }}>
          <Text testID="channels-publish-meta" style={{ fontSize: 12, color: paletteToken("fg_muted") }}>
            {ctf("ch.publishPreviewMeta", {
              tag: state.publishPreviewData()?.tag_name ?? "",
              commit: (state.publishPreviewData()?.commit_sha ?? "").slice(0, 10),
            })}
          </Text>
          <Each
            of={state.publishPreviewData()?.skills ?? []}
            key={(s: { id: string; status: string }) => s.id}
          >
            {(sk: () => { id: string; status: string }) => (
              <View style={{ display: "flex", flexDirection: "row", justifyContent: "space-between", paddingVertical: 3 }}>
                <Text style={{ fontSize: 12, color: paletteToken("fg") }}>{sk().id}</Text>
                <Text style={{ fontSize: 11, color: paletteToken("fg_muted") }}>{sk().status}</Text>
              </View>
            )}
          </Each>
          <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>{ct("ch.releaseTitleLabel")}</Text>
          <Input
            testID="channels-publish-title-input"
            value={state.publishTitle()}
            placeholder={ct("ch.releaseTitlePlaceholder")}
            disabled={state.modalBusy()}
            onChange={(e: { value: string }) => state.publishTitle.set(e.value)}
            style={inputStyle()}
          />
          <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>{ct("ch.releaseNotesLabel")}</Text>
          <Input
            testID="channels-publish-notes-input"
            value={state.publishNotes()}
            placeholder={ct("ch.releaseNotesPlaceholder")}
            disabled={state.modalBusy()}
            onChange={(e: { value: string }) => state.publishNotes.set(e.value)}
            style={inputStyle()}
          />
          <View style={{ display: "flex", flexDirection: "row", justifyContent: "flex-end", gap: 8 }}>
            <ActionButton testID="channels-publish-cancel" label={ct("ch.modalCancel")} onPress={() => void state.closeModal()} />
            <ActionButton
              testID="channels-publish-confirm"
              label={ct("ch.confirmPublishBtn")}
              primary
              disabled={state.modalBusy()}
              onPress={() => void state.confirmPublish()}
            />
          </View>
        </View>
      )}
    </If>
  );
}

function MembersModalBody(props: { state: ChannelsState }): JSX.Element {
  const state = props.state;
  const repoId = () => {
    const active = state.modal();
    return active?.kind === "members" ? active.repositoryId : 0;
  };
  return (
    <View style={{ display: "flex", flexDirection: "column", gap: 10 }}>
      <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 6 }}>
        <View style={{ flex: 1, minWidth: 0 }}>
          <Input
            testID="channels-invite-username-input"
            value={state.inviteUsername()}
            placeholder={ct("ch.inviteUserPlaceholder")}
            disabled={state.modalBusy()}
            onChange={(e: { value: string }) => state.inviteUsername.set(e.value)}
            style={inputStyle()}
          />
        </View>
        <ActionButton
          testID="channels-invite-role-toggle"
          label={state.inviteRole() === "publisher" ? ct("ch.rolePublisher") : ct("ch.roleSubscriber")}
          onPress={() =>
            state.inviteRole.set(state.inviteRole() === "publisher" ? "subscriber" : "publisher")
          }
        />
        <ActionButton
          testID="channels-invite-submit"
          label={ct("ch.inviteBtn")}
          primary
          disabled={state.modalBusy() || state.inviteUsername().trim() === ""}
          onPress={() => void state.submitInvite(repoId())}
        />
      </View>

      <If when={state.membershipData() !== null}>
        {() => (
          <View style={{ display: "flex", flexDirection: "column", gap: 8 }}>
            <Text style={{ fontSize: 12, fontWeight: "600", color: paletteToken("fg") }}>
              {ctf("ch.membersHeading", { count: state.membershipData()?.members.length ?? 0 })}
            </Text>
            <Each
              of={state.membershipData()?.members ?? []}
              key={(m: ChannelMemberRecord) => `${m.user.id}:${m.user.login}`}
            >
              {(m: () => ChannelMemberRecord) => (
                <View style={{ display: "flex", flexDirection: "row", alignItems: "center", justifyContent: "space-between", padding: 6, borderRadius: 6, backgroundColor: paletteToken("card") }}>
                  <Text style={{ fontSize: 12, color: paletteToken("fg") }}>
                    {`@${m().user.login} (${m().role})`}
                  </Text>
                  <If when={m().role !== "owner"}>
                    <ActionButton
                      testID={`channels-revoke-${m().user.login}`}
                      label={ct("ch.revokeMemberBtn")}
                      danger
                      disabled={state.modalBusy()}
                      onPress={() => void state.revokeMember(repoId(), m().user.login)}
                    />
                  </If>
                </View>
              )}
            </Each>

            <Text style={{ fontSize: 12, fontWeight: "600", color: paletteToken("fg") }}>
              {ctf("ch.invitationsHeading", { count: state.membershipData()?.invitations.length ?? 0 })}
            </Text>
            <Each
              of={state.membershipData()?.invitations ?? []}
              key={(inv: ChannelInvitationRecord) => String(inv.id)}
            >
              {(inv: () => ChannelInvitationRecord) => (
                <View style={{ display: "flex", flexDirection: "row", alignItems: "center", justifyContent: "space-between", gap: 6, padding: 6, borderRadius: 6, backgroundColor: paletteToken("card") }}>
                  <Text style={{ fontSize: 12, color: paletteToken("fg"), flex: 1 }}>
                    {`@${inv().invitee?.login ?? "-"} (${inv().role} · ${inv().status})`}
                  </Text>
                  <ActionButton
                    testID={`channels-resend-invite-${inv().id}`}
                    label={ct("ch.resendInviteBtn")}
                    disabled={state.modalBusy()}
                    onPress={() => void state.resendInvite(repoId(), inv().id)}
                  />
                  <ActionButton
                    testID={`channels-cancel-invite-${inv().id}`}
                    label={ct("ch.cancelInviteBtn")}
                    danger
                    disabled={state.modalBusy()}
                    onPress={() => void state.cancelInvite(repoId(), inv().id)}
                  />
                </View>
              )}
            </Each>
          </View>
        )}
      </If>
    </View>
  );
}

function RollbackModalBody(props: { state: ChannelsState }): JSX.Element {
  const state = props.state;
  return (
    <View style={{ display: "flex", flexDirection: "column", gap: 10 }}>
      <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>{ct("ch.rollbackHint")}</Text>
      <Each of={state.rollbackTargets()} key={(t: RollbackTargetItem) => String(t.target.revision)}>
        {(item: () => RollbackTargetItem) => (
          <View
            testID={`channels-rollback-rev-${item().target.revision}`}
            style={{
              display: "flex",
              flexDirection: "row",
              alignItems: "center",
              justifyContent: "space-between",
              padding: 8,
              borderRadius: 8,
              cursor: "pointer",
              borderWidth: 1,
              borderColor:
                state.rollbackRevision() === item().target.revision
                  ? paletteToken("accent")
                  : paletteToken("border"),
              backgroundColor: paletteToken("card"),
            }}
            onPress={() => state.rollbackRevision.set(item().target.revision)}
          >
            <Text style={{ fontSize: 12, color: paletteToken("fg") }}>
              {`${item().target.tag_name} · ${item().title}`}
            </Text>
            <Text style={{ fontSize: 11, color: paletteToken("fg_muted") }}>{item().published_at}</Text>
          </View>
        )}
      </Each>

      <View style={{ display: "flex", flexDirection: "row", flexWrap: "wrap", gap: 6 }}>
        <ActionButton
          testID="channels-rollback-div-none"
          label={ct("ch.rollbackConflictNone")}
          primary={state.rollbackDivergence() === "none"}
          onPress={() => state.rollbackDivergence.set("none")}
        />
        <ActionButton
          testID="channels-rollback-div-preserve"
          label={ctf("ch.conflictPreserve", {
            name: `${state.modal()?.kind === "rollback" ? (state.modal() as { skillId?: string } | null)?.skillId ?? "skill" : "skill"}.local`,
          })}
          primary={state.rollbackDivergence() === "preserve"}
          onPress={() => state.rollbackDivergence.set("preserve")}
        />
        <ActionButton
          testID="channels-rollback-div-discard"
          label={ct("ch.conflictDiscard")}
          primary={state.rollbackDivergence() === "discard"}
          onPress={() => state.rollbackDivergence.set("discard")}
        />
      </View>

      <View style={{ display: "flex", flexDirection: "row", justifyContent: "flex-end", gap: 8 }}>
        <ActionButton testID="channels-rollback-cancel" label={ct("ch.modalCancel")} onPress={() => void state.closeModal()} />
        <ActionButton
          testID="channels-rollback-confirm"
          label={ct("ch.rollbackConfirmBtn")}
          primary
          disabled={state.modalBusy() || state.rollbackRevision() === null}
          onPress={() => void state.confirmRollback()}
        />
      </View>
    </View>
  );
}

function ConvertLocalModalBody(props: { state: ChannelsState }): JSX.Element {
  const state = props.state;
  return (
    <View style={{ display: "flex", flexDirection: "column", gap: 10 }}>
      <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>{ct("ch.convertTargetLabel")}</Text>
      <Input
        testID="channels-convert-name-input"
        value={state.convertLocalName()}
        disabled={state.modalBusy()}
        onChange={(e: { value: string }) => state.convertLocalName.set(e.value)}
        style={inputStyle()}
      />
      <View style={{ display: "flex", flexDirection: "row", justifyContent: "flex-end", gap: 8 }}>
        <ActionButton testID="channels-convert-cancel" label={ct("ch.modalCancel")} onPress={() => void state.closeModal()} />
        <ActionButton
          testID="channels-convert-confirm"
          label={ct("ch.convertConfirmBtn")}
          primary
          disabled={state.modalBusy() || state.convertLocalName().trim() === ""}
          onPress={() => void state.confirmConvertLocal()}
        />
      </View>
    </View>
  );
}

function ExportMarketplaceBody(props: { state: ChannelsState }): JSX.Element {
  const state = props.state;
  return (
    <View style={{ display: "flex", flexDirection: "column", gap: 10 }}>
      <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>{ct("ch.exportOutLabel")}</Text>
      <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 8 }}>
        <View style={{ flex: 1, minWidth: 0 }}>
          <Input
            testID="channels-export-dir-input"
            value={state.exportOutDir()}
            placeholder={ct("ch.exportOutPlaceholder")}
            disabled={state.modalBusy()}
            onChange={(e: { value: string }) => state.exportOutDir.set(e.value)}
            style={inputStyle()}
          />
        </View>
        <ActionButton
          testID="channels-export-browse"
          label={ct("ch.exportBrowseBtn")}
          disabled={state.modalBusy()}
          onPress={() => void state.browseExportDir()}
        />
      </View>
      <View style={{ display: "flex", flexDirection: "row", justifyContent: "flex-end", gap: 8 }}>
        <ActionButton testID="channels-export-cancel" label={ct("ch.modalCancel")} onPress={() => void state.closeModal()} />
        <ActionButton
          testID="channels-export-confirm"
          label={ct("ch.exportConfirmBtn")}
          primary
          disabled={state.modalBusy() || state.exportOutDir().trim() === ""}
          onPress={() => void state.confirmExport()}
        />
      </View>
    </View>
  );
}
