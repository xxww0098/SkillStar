import {
  Clipboard,
  Overlay,
  Text,
  View,
  computed,
  effect,
  openUrl,
  signal,
  untracked,
  type Accessor,
  type JSX,
} from "nsh";
import { Input, Switch } from "nsh/base";
import { MySkillsGitHub } from "skillstar-gui/native";
import { Icon } from "../../chrome/icons";
import { IconSpin } from "../../chrome/feedback";
import { promptVeil, SHEET_SHADOW } from "../../chrome/dialog";
import { fadeTransition, settleTransition } from "../../chrome/motion";
import { isLight, paletteToken } from "../../services/themeMode";
import { openGitHubSignIn } from "./githubSignInSlot";
import { github } from "./state";
import { pt, ptf } from "./publishStrings";

export interface GitInstallInstruction {
  label: string;
  command: string;
}

export interface PublishPreflightInfo {
  status: "not_installed" | "not_authenticated" | "ready";
  username: string | null;
  authState: string;
  gitInstalled: boolean;
  authenticated: boolean;
  gitDownloadUrl: string | null;
  gitInstallInstructions: GitInstallInstruction[];
  skillName: string;
  skillExists: boolean;
  suggestedRepo: string;
  suggestedDescription: string;
}

export interface PublishOutcome {
  url: string;
  gitUrl: string;
  sourceFolder: string;
  isPrivate: boolean;
}

export interface PublishBridge {
  preflight(skillName: string): Promise<PublishPreflightInfo>;
  publishSkill(
    skillName: string,
    repoName: string,
    isPrivate: boolean,
    description: string
  ): Promise<PublishOutcome>;
  openSignIn?(): void;
}

const nativeBridge: PublishBridge = {
  preflight: (skillName) =>
    MySkillsGitHub.publishPreflight(skillName) as Promise<PublishPreflightInfo>,
  publishSkill: (skillName, repoName, isPrivate, description) =>
    MySkillsGitHub.publishSkill(
      skillName,
      repoName,
      isPrivate,
      description
    ) as Promise<PublishOutcome>,
  openSignIn: openGitHubSignIn,
};

let activeBridge: PublishBridge = nativeBridge;

type PublishPhase =
  | { kind: "checking"; skillName: string }
  | { kind: "not_installed"; info: PublishPreflightInfo }
  | { kind: "not_authenticated"; info: PublishPreflightInfo }
  | { kind: "ready"; info: PublishPreflightInfo }
  | { kind: "publishing"; skillName: string }
  | { kind: "done"; outcome: PublishOutcome }
  | { kind: "error"; message: string; info: PublishPreflightInfo | null };

const targetSkill = signal<string | null>(null);
const phase = signal<PublishPhase | null>(null);
const repoName = signal("");
const isPrivate = signal(true);
const description = signal("");
const copiedUrl = signal(false);
let flowEpoch = 0;

export function isValidRepoName(name: string): boolean {
  const trimmed = name.trim();
  if (trimmed.length === 0 || trimmed.length > 100 || trimmed.startsWith(".")) return false;
  return /^[A-Za-z0-9._-]+$/.test(trimmed);
}

export function openPublishSkill(skillName: string): void {
  const trimmed = skillName.trim();
  if (trimmed.length === 0) return;
  targetSkill.set(trimmed);
  void runPreflight(trimmed);
}

export function closePublishSkill(force = false): void {
  const current = phase();
  if (!force && current?.kind === "publishing") return;
  flowEpoch += 1;
  targetSkill.set(null);
  phase.set(null);
  copiedUrl.set(false);
}

async function runPreflight(skillName: string): Promise<void> {
  const epoch = ++flowEpoch;
  copiedUrl.set(false);
  phase.set({ kind: "checking", skillName });
  try {
    const info = await activeBridge.preflight(skillName);
    if (epoch !== flowEpoch || targetSkill() !== skillName) return;
    repoName.set(info.suggestedRepo || skillName);
    description.set(info.suggestedDescription || "");
    if (info.status === "not_installed") {
      phase.set({ kind: "not_installed", info });
    } else if (info.status === "not_authenticated") {
      phase.set({ kind: "not_authenticated", info });
    } else {
      phase.set({ kind: "ready", info });
    }
  } catch (err: unknown) {
    if (epoch !== flowEpoch || targetSkill() !== skillName) return;
    phase.set({
      kind: "error",
      message: ptf("publish.preflightFailed", {
        error: err instanceof Error ? err.message : String(err),
      }),
      info: null,
    });
  }
}

async function submitPublish(info: PublishPreflightInfo): Promise<void> {
  if (!isValidRepoName(repoName())) return;
  const epoch = ++flowEpoch;
  const skill = info.skillName;
  const repo = repoName().trim();
  const priv = isPrivate();
  const desc = description().trim();
  phase.set({ kind: "publishing", skillName: skill });
  try {
    const outcome = await activeBridge.publishSkill(skill, repo, priv, desc);
    if (epoch !== flowEpoch) return;
    phase.set({ kind: "done", outcome });
  } catch (err: unknown) {
    if (epoch !== flowEpoch) return;
    phase.set({
      kind: "error",
      message: err instanceof Error ? err.message : String(err),
      info,
    });
  }
}

// 用户在登录浮层完成设备授权后自动重跑预检。
effect(() => {
  const session = github();
  const current = untracked(phase);
  const name = untracked(targetSkill);
  if (session?.state === "connected" && current?.kind === "not_authenticated" && name !== null) {
    untracked(() => void runPreflight(name));
  }
});

export function PublishModalHost(props: { bridge?: PublishBridge }): JSX.Element | null {
  activeBridge = props.bridge ?? nativeBridge;
  return (
    <If when={targetSkill() !== null && phase() !== null}>
      {() => <PublishDialog />}
    </If>
  );
}

function PublishDialog(): JSX.Element {
  return (
    <Overlay
      style={{ position: "fixed", top: 0, left: 0, right: 0, bottom: 0, zIndex: 60 }}
      onCancel={() => closePublishSkill()}
    >
      <View
        testID="publish-modal-veil"
        style={{
          position: "absolute",
          top: 0,
          left: 0,
          right: 0,
          bottom: 0,
          backgroundColor: promptVeil(),
        }}
        onPress={() => closePublishSkill()}
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
        }}
      >
        <View
          testID="publish-modal-card"
          style={{
            display: "flex",
            flexDirection: "column",
            width: 460,
            borderRadius: 12,
            borderWidth: 1,
            borderColor: paletteToken("border"),
            backgroundColor: isLight() ? paletteToken("card") : paletteToken("panel"),
            boxShadow: SHEET_SHADOW,
            ...fadeTransition(true),
          }}
          onPress={() => {
            // 卡片吞掉按压，避免误触发 veil 关闭。
          }}
        >
          <ModalHeader />
          <ModalBody />
        </View>
      </View>
    </Overlay>
  );
}

function ModalHeader(): JSX.Element {
  const skillLabel = computed(() =>
    ptf("publish.skillLabel", { name: targetSkill() ?? "" })
  );
  return (
    <View
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        justifyContent: "space-between",
        gap: 10,
        paddingHorizontal: 24,
        paddingTop: 16,
        paddingBottom: 12,
        borderBottomWidth: 1,
        borderBottomColor: paletteToken("border_soft"),
      }}
    >
      <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 10, minWidth: 0 }}>
        <View
          style={{
            width: 32,
            height: 32,
            borderRadius: 10,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            backgroundColor: paletteToken("accent_soft"),
          }}
        >
          <Icon name="gitBranch" size={16} color={paletteToken("accent")} />
        </View>
        <View style={{ display: "flex", flexDirection: "column", gap: 2, minWidth: 0 }}>
          <Text style={{ fontSize: 15, fontWeight: "600", color: paletteToken("fg") }}>
            {pt("publish.title")}
          </Text>
          <Text style={{ fontSize: 12, color: paletteToken("fg_muted"), numberOfLines: 1 }}>
            {skillLabel()}
          </Text>
        </View>
      </View>
      <View
        testID="publish-modal-close"
        style={{
          width: 28,
          height: 28,
          borderRadius: 6,
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          cursor: "pointer",
        }}
        hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
        onPress={() => closePublishSkill()}
      >
        <Icon name="x" size={14} color={paletteToken("fg_muted")} />
      </View>
    </View>
  );
}

function ModalBody(): JSX.Element {
  return (
    <View style={{ display: "flex", flexDirection: "column", paddingHorizontal: 24, paddingVertical: 16, gap: 14 }}>
      <If when={phase()?.kind === "checking"}>
        <View testID="publish-phase-checking" style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 8, paddingVertical: 12 }}>
          <IconSpin name="loader" size={14} color={paletteToken("fg_muted")} spin defer={false} />
          <Text style={{ fontSize: 13, color: paletteToken("fg_muted") }}>{pt("publish.checking")}</Text>
        </View>
      </If>
      <If when={phase()?.kind === "not_installed"}>
        <GitMissingView />
      </If>
      <If when={phase()?.kind === "not_authenticated"}>
        <AuthRequiredView />
      </If>
      <If when={phase()?.kind === "ready"}>
        <ReadyFormView />
      </If>
      <If when={phase()?.kind === "publishing"}>
        <View testID="publish-phase-publishing" style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 8, paddingVertical: 16 }}>
          <IconSpin name="loader" size={14} color={paletteToken("accent")} spin defer={false} />
          <Text style={{ fontSize: 13, color: paletteToken("fg") }}>{pt("publish.submitting")}</Text>
        </View>
      </If>
      <If when={phase()?.kind === "done"}>
        <DoneView />
      </If>
      <If when={phase()?.kind === "error"}>
        <ErrorView />
      </If>
    </View>
  );
}

function GitMissingView(): JSX.Element {
  const info = () => {
    const p = phase();
    return p?.kind === "not_installed" ? p.info : null;
  };
  return (
    <View testID="publish-phase-not-installed" style={{ display: "flex", flexDirection: "column", gap: 12 }}>
      <View
        style={{
          display: "flex",
          flexDirection: "column",
          gap: 6,
          padding: 12,
          borderRadius: 8,
          backgroundColor: paletteToken("warn_bg"),
          borderWidth: 1,
          borderColor: paletteToken("warn_border"),
        }}
      >
        <Text style={{ fontSize: 13, fontWeight: "600", color: paletteToken("warn") }}>
          {pt("publish.gitMissingTitle")}
        </Text>
        <Text style={{ fontSize: 12, color: paletteToken("fg") }}>{pt("publish.gitMissingBody")}</Text>
      </View>
      <Each of={info()?.gitInstallInstructions ?? []} key={(item) => item.label}>
        {(item: Accessor<GitInstallInstruction>) => (
          <View
            style={{
              display: "flex",
              flexDirection: "row",
              alignItems: "center",
              justifyContent: "space-between",
              gap: 8,
              paddingHorizontal: 10,
              paddingVertical: 8,
              borderRadius: 8,
              backgroundColor: paletteToken("card"),
              borderWidth: 1,
              borderColor: paletteToken("border"),
            }}
          >
            <View style={{ display: "flex", flexDirection: "column", gap: 2, minWidth: 0, flex: 1 }}>
              <Text style={{ fontSize: 11, color: paletteToken("fg_muted") }}>{item().label}</Text>
              <Text style={{ fontSize: 12, fontFamily: "monospace", color: paletteToken("fg") }}>
                {item().command}
              </Text>
            </View>
          </View>
        )}
      </Each>
      <View style={{ display: "flex", flexDirection: "row", justifyContent: "flex-end", gap: 8 }}>
        <If when={typeof info()?.gitDownloadUrl === "string"}>
          <GhostBtn
            testID="publish-git-download"
            label={pt("publish.gitDownload")}
            onPress={() => {
              const url = info()?.gitDownloadUrl;
              if (url) void openUrl(url);
            }}
          />
        </If>
        <PrimaryBtn
          testID="publish-recheck"
          label={pt("publish.recheck")}
          onPress={() => {
            const name = targetSkill();
            if (name) void runPreflight(name);
          }}
        />
      </View>
    </View>
  );
}

function AuthRequiredView(): JSX.Element {
  const isExpired = () => {
    const p = phase();
    return p?.kind === "not_authenticated" && p.info.authState === "expired";
  };
  return (
    <View testID="publish-phase-not-authenticated" style={{ display: "flex", flexDirection: "column", gap: 12 }}>
      <View
        style={{
          display: "flex",
          flexDirection: "column",
          gap: 6,
          padding: 12,
          borderRadius: 8,
          backgroundColor: paletteToken("info_bg"),
          borderWidth: 1,
          borderColor: paletteToken("info_border"),
        }}
      >
        <Text style={{ fontSize: 13, fontWeight: "600", color: paletteToken("info") }}>
          {pt("publish.authRequiredTitle")}
        </Text>
        <Text style={{ fontSize: 12, color: paletteToken("fg") }}>
          {isExpired() ? pt("publish.authExpiredBody") : pt("publish.authSignedOutBody")}
        </Text>
        <Text style={{ fontSize: 11, color: paletteToken("fg_muted") }}>
          {ptf("publish.permissionNotice", { name: targetSkill() ?? "" })}
        </Text>
      </View>
      <View style={{ display: "flex", flexDirection: "row", justifyContent: "flex-end", gap: 8 }}>
        <GhostBtn
          testID="publish-recheck"
          label={pt("publish.recheck")}
          onPress={() => {
            const name = targetSkill();
            if (name) void runPreflight(name);
          }}
        />
        <PrimaryBtn
          testID="publish-sign-in"
          label={pt("publish.signInBtn")}
          onPress={() => (activeBridge.openSignIn ?? openGitHubSignIn)()}
        />
      </View>
    </View>
  );
}

function ReadyFormView(): JSX.Element {
  const info = () => {
    const p = phase();
    return p?.kind === "ready" ? p.info : null;
  };
  const valid = computed(() => isValidRepoName(repoName()));
  return (
    <View testID="publish-phase-ready" style={{ display: "flex", flexDirection: "column", gap: 12 }}>
      <View
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          justifyContent: "space-between",
          paddingHorizontal: 10,
          paddingVertical: 8,
          borderRadius: 8,
          backgroundColor: paletteToken("ok_bg"),
          borderWidth: 1,
          borderColor: paletteToken("ok_border"),
        }}
      >
        <Text testID="publish-ready-user" style={{ fontSize: 12, fontWeight: "600", color: paletteToken("ok") }}>
          {ptf("publish.readyBadge", { username: info()?.username ?? "github" })}
        </Text>
      </View>

      <View style={{ display: "flex", flexDirection: "column", gap: 6 }}>
        <Text style={{ fontSize: 12, fontWeight: "500", color: paletteToken("fg") }}>
          {pt("publish.repoNameLabel")}
        </Text>
        <Input
          testID="publish-repo-input"
          value={repoName()}
          placeholder={pt("publish.repoNamePlaceholder")}
          style={{
            width: "100%",
            height: 32,
            paddingHorizontal: 10,
            borderRadius: 6,
            borderWidth: 1,
            borderColor: valid() ? paletteToken("border") : paletteToken("danger"),
            backgroundColor: paletteToken("input"),
            fontSize: 13,
            color: paletteToken("fg"),
            placeholderColor: paletteToken("fg_muted"),
          }}
          onChange={(event: { value: string }) => repoName.set(event.value)}
        />
        <If when={!valid()}>
          <Text testID="publish-repo-error" style={{ fontSize: 11, color: paletteToken("danger") }}>
            {pt("publish.repoNameError")}
          </Text>
        </If>
      </View>

      <View
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          justifyContent: "space-between",
          paddingVertical: 4,
        }}
      >
        <Text style={{ fontSize: 12, fontWeight: "500", color: paletteToken("fg") }}>
          {pt("publish.visibilityLabel")}
        </Text>
        <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 8 }}>
          <Text testID="publish-visibility-label" style={{ fontSize: 12, color: paletteToken("fg_muted") }}>
            {isPrivate() ? pt("publish.privateRepo") : pt("publish.publicRepo")}
          </Text>
          <Switch
            testID="publish-private-switch"
            checked={isPrivate()}
            accessibilityLabel={pt("publish.visibilityLabel")}
            trackStyle={{
              width: 30,
              height: 16,
              borderRadius: 8,
              borderWidth: 1,
              backgroundColor: isPrivate() ? paletteToken("accent") : paletteToken("input"),
              borderColor: isPrivate() ? paletteToken("accent") : paletteToken("edge"),
            }}
            thumbStyle={{
              width: 12,
              height: 12,
              borderRadius: 6,
              backgroundColor: paletteToken("on_accent"),
              transform: isPrivate() ? "translateX(14px)" : "translateX(0px)",
              ...settleTransition(),
            }}
            onChange={() => isPrivate.set(!isPrivate())}
          />
        </View>
      </View>

      <View style={{ display: "flex", flexDirection: "column", gap: 6 }}>
        <Text style={{ fontSize: 12, fontWeight: "500", color: paletteToken("fg") }}>
          {pt("publish.descriptionLabel")}
        </Text>
        <Input
          testID="publish-desc-input"
          value={description()}
          placeholder={pt("publish.descriptionPlaceholder")}
          style={{
            width: "100%",
            height: 32,
            paddingHorizontal: 10,
            borderRadius: 6,
            borderWidth: 1,
            borderColor: paletteToken("border"),
            backgroundColor: paletteToken("input"),
            fontSize: 13,
            color: paletteToken("fg"),
            placeholderColor: paletteToken("fg_muted"),
          }}
          onChange={(event: { value: string }) => description.set(event.value)}
        />
      </View>

      <Text style={{ fontSize: 11, color: paletteToken("fg_muted") }}>
        {ptf("publish.permissionNotice", { name: info()?.skillName ?? "" })}
      </Text>

      <View style={{ display: "flex", flexDirection: "row", justifyContent: "flex-end", gap: 8 }}>
        <GhostBtn testID="publish-cancel" label={pt("publish.cancel")} onPress={() => closePublishSkill(true)} />
        <PrimaryBtn
          testID="publish-submit"
          label={pt("publish.submit")}
          disabled={!valid()}
          onPress={() => {
            const currentInfo = info();
            if (currentInfo) void submitPublish(currentInfo);
          }}
        />
      </View>
    </View>
  );
}

function DoneView(): JSX.Element {
  const outcome = () => {
    const p = phase();
    return p?.kind === "done" ? p.outcome : null;
  };
  return (
    <View testID="publish-phase-done" style={{ display: "flex", flexDirection: "column", gap: 12 }}>
      <View
        style={{
          display: "flex",
          flexDirection: "column",
          gap: 6,
          padding: 12,
          borderRadius: 8,
          backgroundColor: paletteToken("ok_bg"),
          borderWidth: 1,
          borderColor: paletteToken("ok_border"),
        }}
      >
        <Text style={{ fontSize: 13, fontWeight: "600", color: paletteToken("ok") }}>
          {pt("publish.doneTitle")}
        </Text>
        <Text testID="publish-done-url" style={{ fontSize: 12, fontFamily: "monospace", color: paletteToken("fg") }}>
          {outcome()?.url ?? ""}
        </Text>
        <Text style={{ fontSize: 11, color: paletteToken("fg_muted") }}>
          {ptf("publish.doneFolder", { folder: outcome()?.sourceFolder ?? "" })}
        </Text>
      </View>
      <View style={{ display: "flex", flexDirection: "row", justifyContent: "flex-end", gap: 8 }}>
        <GhostBtn
          testID="publish-copy-url"
          label={copiedUrl() ? pt("publish.copied") : pt("publish.copyUrl")}
          onPress={() => {
            const url = outcome()?.url;
            if (!url) return;
            void Clipboard.writeText(url).then(() => copiedUrl.set(true));
          }}
        />
        <GhostBtn
          testID="publish-open-url"
          label={pt("publish.openRepo")}
          onPress={() => {
            const url = outcome()?.url;
            if (url) void openUrl(url);
          }}
        />
        <PrimaryBtn testID="publish-done-close" label={pt("publish.close")} onPress={() => closePublishSkill(true)} />
      </View>
    </View>
  );
}

function ErrorView(): JSX.Element {
  const errState = () => {
    const p = phase();
    return p?.kind === "error" ? p : null;
  };
  return (
    <View testID="publish-phase-error" style={{ display: "flex", flexDirection: "column", gap: 12 }}>
      <View
        style={{
          padding: 12,
          borderRadius: 8,
          backgroundColor: paletteToken("danger_bg"),
          borderWidth: 1,
          borderColor: paletteToken("danger_border"),
        }}
      >
        <Text testID="publish-error-msg" style={{ fontSize: 12, color: paletteToken("danger_fg") }}>
          {errState()?.message ?? ""}
        </Text>
      </View>
      <View style={{ display: "flex", flexDirection: "row", justifyContent: "flex-end", gap: 8 }}>
        <GhostBtn testID="publish-cancel" label={pt("publish.cancel")} onPress={() => closePublishSkill(true)} />
        <PrimaryBtn
          testID="publish-retry"
          label={pt("publish.retry")}
          onPress={() => {
            const current = errState();
            if (current?.info) {
              phase.set({ kind: "ready", info: current.info });
              return;
            }
            const name = targetSkill();
            if (name) void runPreflight(name);
          }}
        />
      </View>
    </View>
  );
}

function GhostBtn(props: { testID: string; label: string; onPress: () => void }): JSX.Element {
  return (
    <View
      testID={props.testID}
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        height: 30,
        paddingHorizontal: 14,
        borderRadius: 6,
        borderWidth: 1,
        borderColor: paletteToken("border"),
        cursor: "pointer",
      }}
      hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
      onPress={props.onPress}
    >
      <Text style={{ fontSize: 12, color: paletteToken("fg") }}>{props.label}</Text>
    </View>
  );
}

function PrimaryBtn(props: {
  testID: string;
  label: string;
  disabled?: boolean;
  onPress: () => void;
}): JSX.Element {
  return (
    <View
      testID={props.testID}
      disabled={props.disabled === true}
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        height: 30,
        paddingHorizontal: 16,
        borderRadius: 6,
        backgroundColor: paletteToken("accent"),
        opacity: props.disabled === true ? 0.5 : 1,
        cursor: props.disabled === true ? "default" : "pointer",
      }}
      hoverStyle={props.disabled === true ? undefined : { backgroundColor: paletteToken("accent_hover") }}
      onPress={() => {
        if (props.disabled !== true) props.onPress();
      }}
    >
      <Text style={{ fontSize: 12, fontWeight: "500", color: paletteToken("on_accent") }}>
        {props.label}
      </Text>
    </View>
  );
}
