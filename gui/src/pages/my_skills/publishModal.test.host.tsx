import { Button } from "nsh/base";
import { Text, View, host, mount, signal, type JSX } from "nsh";
import {
  PublishModalHost,
  closePublishSkill,
  openPublishSkill,
  type PublishBridge,
  type PublishOutcome,
  type PublishPreflightInfo,
} from "./publishModal";

const preflightMode = signal<"not_installed" | "not_authenticated" | "ready">("ready");
const publishCalls = signal(0);
const signInTriggered = signal(0);
const shouldFailPublish = signal(false);

const bridge: PublishBridge = {
  preflight: async (skillName): Promise<PublishPreflightInfo> => {
    const mode = preflightMode();
    if (mode === "not_installed") {
      return {
        status: "not_installed",
        username: null,
        authState: "signed_out",
        gitInstalled: false,
        authenticated: false,
        gitDownloadUrl: "https://git-scm.com/downloads/mac",
        gitInstallInstructions: [{ label: "Homebrew", command: "brew install git" }],
        skillName,
        skillExists: true,
        suggestedRepo: skillName,
        suggestedDescription: "Draft desc",
      };
    }
    if (mode === "not_authenticated") {
      return {
        status: "not_authenticated",
        username: null,
        authState: "expired",
        gitInstalled: true,
        authenticated: false,
        gitDownloadUrl: null,
        gitInstallInstructions: [],
        skillName,
        skillExists: true,
        suggestedRepo: skillName,
        suggestedDescription: "Draft desc",
      };
    }
    return {
      status: "ready",
      username: "octocat",
      authState: "connected",
      gitInstalled: true,
      authenticated: true,
      gitDownloadUrl: null,
      gitInstallInstructions: [],
      skillName,
      skillExists: true,
      suggestedRepo: skillName,
      suggestedDescription: "Local skill description",
    };
  },
  publishSkill: async (skillName, repoName, isPrivate): Promise<PublishOutcome> => {
    publishCalls.set(publishCalls() + 1);
    if (shouldFailPublish()) {
      throw new Error("Repository name already exists");
    }
    return {
      url: `https://github.com/octocat/${repoName}`,
      gitUrl: `https://github.com/octocat/${repoName}.git`,
      sourceFolder: `skills/${skillName}`,
      isPrivate,
    };
  },
  openSignIn: () => {
    signInTriggered.set(signInTriggered() + 1);
  },
};

host.on("harness", (payload) => {
  const cmd = payload as { action?: string; mode?: "not_installed" | "not_authenticated" | "ready"; fail?: boolean };
  if (cmd.action === "set-mode" && cmd.mode) {
    preflightMode.set(cmd.mode);
  } else if (cmd.action === "set-fail") {
    shouldFailPublish.set(cmd.fail === true);
  } else if (cmd.action === "open") {
    openPublishSkill("my-local-skill");
  } else if (cmd.action === "close") {
    closePublishSkill(true);
  }
});

function App(): JSX.Element {
  return (
    <View style={{ display: "flex", flexDirection: "column", height: "100%", gap: 8 }}>
      <Button testID="open-btn" onPress={() => openPublishSkill("my-local-skill")}>
        <Text>open</Text>
      </Button>
      <Text testID="publish-host-counters">
        {`calls:${publishCalls()} signIn:${signInTriggered()}`}
      </Text>
      <View style={{ flex: 1 }}>
        <PublishModalHost bridge={bridge} />
      </View>
    </View>
  );
}

mount(App);
