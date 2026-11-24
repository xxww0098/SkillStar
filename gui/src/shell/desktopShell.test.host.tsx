// desktopShell 退出确认与 ShellErrorFallback 的 headless 测试宿主。
import { Button } from "nsh/base";
import { ErrorBoundary, Text, View, mount, signal, type JSX } from "nsh";
import { DialogHost } from "../chrome/dialog";
import { ShellErrorFallback } from "./Shell";
import { requestAppQuit } from "./desktopShell";
import { dropJob, putJob } from "./jobBoard";

const cancelCount = signal(0);
const shouldThrow = signal(false);

function ThrowProbe(): JSX.Element {
  return (
    <Text testID="recovered-child">
      {() => {
        if (shouldThrow()) {
          throw new Error("boom in page render");
        }
        return "recovered-child";
      }}
    </Text>
  );
}

function DesktopShellTestHost(): JSX.Element {
  return (
    <View style={{ width: "100%", height: "100%", display: "flex", flexDirection: "column", gap: 8, padding: 16 }}>
      <View style={{ display: "flex", flexDirection: "row", gap: 8 }}>
        <Button
          testID="host-add-job"
          onPress={() => {
            putJob({
              id: "job-1",
              title: "Syncing skills",
              cancellable: true,
              onCancel: () => cancelCount.set(cancelCount() + 1),
            });
          }}
        >
          <Text>Add job</Text>
        </Button>
        <Button testID="host-drop-job" onPress={() => dropJob("job-1")}>
          <Text>Drop job</Text>
        </Button>
        <Button testID="host-quit" onPress={() => requestAppQuit(null)}>
          <Text>Quit</Text>
        </Button>
        <Button testID="host-crash-page" onPress={() => shouldThrow.set(true)}>
          <Text>Crash</Text>
        </Button>
        <Button testID="host-fix-page" onPress={() => shouldThrow.set(false)}>
          <Text>Fix</Text>
        </Button>
      </View>

      <Text testID="host-cancel-count">{() => `cancel-count:${cancelCount()}`}</Text>

      <View style={{ flex: 1, minHeight: 160 }}>
        <ErrorBoundary
          fallback={(error, reset) => (
            <ShellErrorFallback page="projects" error={error} onReset={reset} />
          )}
        >
          <ThrowProbe />
        </ErrorBoundary>
      </View>

      <DialogHost />
    </View>
  );
}

mount(() => <DesktopShellTestHost />);
