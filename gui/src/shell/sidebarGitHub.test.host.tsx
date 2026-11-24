import { Button } from "nsh/base";
import { Text, View, computed, mount, signal, type JSX } from "nsh";
import { SidebarGitHubEntry } from "./SidebarGitHubEntry";
import type { SidebarGitHubBridge } from "./githubAuthBridge";

const collapsedSig = signal(false);
const stateSig = signal<"signed_out" | "connected" | "expired">("signed_out");
const loginSig = signal<string | null>(null);
const pollingSig = signal(false);
const userCodeSig = signal<string | null>(null);

const calls = { signIn: 0, refresh: 0, logout: 0, openChannels: 0 };
const pulse = signal(0);
const bump = () => pulse.set(pulse() + 1);

const fakeBridge: SidebarGitHubBridge = {
  summary: computed(() => ({
    state: stateSig(),
    login: loginSig(),
    polling: pollingSig(),
    userCode: userCodeSig(),
  })),
  polling: () => pollingSig(),
  openSignIn() {
    calls.signIn += 1;
    bump();
  },
  async refreshAuth() {
    calls.refresh += 1;
    stateSig.set("connected");
    loginSig.set("octocat");
    bump();
  },
  async logout() {
    calls.logout += 1;
    stateSig.set("signed_out");
    loginSig.set(null);
    pollingSig.set(false);
    bump();
  },
  openChannelsWorkspace() {
    calls.openChannels += 1;
    bump();
  },
};

function App(): JSX.Element {
  return (
    <View style={{ display: "flex", flexDirection: "column", gap: 10, padding: 12, width: 240 }}>
      <View style={{ display: "flex", flexDirection: "row", flexWrap: "wrap", gap: 6 }}>
        <Button
          testID="host-signed-out"
          onPress={() => {
            stateSig.set("signed_out");
            loginSig.set(null);
            pollingSig.set(false);
            userCodeSig.set(null);
          }}
        >
          <Text>signed_out</Text>
        </Button>
        <Button
          testID="host-polling"
          onPress={() => {
            stateSig.set("signed_out");
            pollingSig.set(true);
            userCodeSig.set("ABCD-1234");
          }}
        >
          <Text>polling</Text>
        </Button>
        <Button
          testID="host-connected"
          onPress={() => {
            stateSig.set("connected");
            loginSig.set("octocat");
            pollingSig.set(false);
            userCodeSig.set(null);
          }}
        >
          <Text>connected</Text>
        </Button>
        <Button
          testID="host-expired"
          onPress={() => {
            stateSig.set("expired");
            loginSig.set("octocat");
            pollingSig.set(false);
            userCodeSig.set(null);
          }}
        >
          <Text>expired</Text>
        </Button>
        <Button testID="host-collapse" onPress={() => collapsedSig.set(!collapsedSig())}>
          <Text>collapse</Text>
        </Button>
      </View>

      <Text testID="host-calls">
        {() => {
          void pulse();
          return `signIn:${calls.signIn} refresh:${calls.refresh} logout:${calls.logout} ch:${calls.openChannels}`;
        }}
      </Text>

      <View style={{ width: collapsedSig() ? 56 : 180 }}>
        <SidebarGitHubEntry collapsed={collapsedSig()} bridge={fakeBridge} />
      </View>
    </View>
  );
}

export default App;
mount(App);
