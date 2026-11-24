import { Text, View, computed, effect, signal, type JSX, type Signal } from "nsh";
import { Icon } from "../chrome/icons";
import { IconSpin } from "../chrome/feedback";
import { SLOW_MS } from "../chrome/delayedFlag";
import { TOOLBAR_H } from "../layout";
import { paletteToken } from "../services/themeMode";
import { t } from "../services/translation";
import { dropJob, jobList, type Job } from "./jobBoard";

// Top-right job stack. The clock ticks so the "taking longer" line can
// appear at 10s without each caller owning a timer. Read `now` in JSX.
// The band clears the toolbar. The cards themselves are unchanged.

const CARD_W = 320;

export function JobBand(): JSX.Element {
  const now = signal(Date.now());
  const hasActiveJob = computed(() => jobList().some((job) => !job.done));
  effect(() => {
    if (!hasActiveJob()) return;
    now.set(Date.now());
    const id = setInterval(() => now.set(Date.now()), 1000);
    return () => clearInterval(id);
  });
  return (
    <If when={jobList().length > 0}>
      <View
        testID="job-band"
        style={{
          position: "absolute",
          top: TOOLBAR_H + 12,
          right: 12,
          width: CARD_W,
          display: "flex",
          flexDirection: "column",
          gap: 8,
          zIndex: 40,
          pointerEvents: "box-none",
        }}
      >
        <Each of={jobList()} key={(job) => job.id}>
          {(job) => <JobCard job={job} now={now} />}
        </Each>
      </View>
    </If>
  );
}

function JobCard(props: { job: () => Job; now: Signal<number> }): JSX.Element {
  return (
    <View
      testID={`job-card-${props.job().id}`}
      accessibilityState={{ busy: !props.job().done }}
      style={{
        width: CARD_W,
        borderRadius: 16,
        borderWidth: 1,
        borderColor: paletteToken("border"),
        backgroundColor: paletteToken("card"),
        padding: 12,
        display: "flex",
        flexDirection: "column",
        gap: 8,
      }}
    >
      <View style={{ display: "flex", flexDirection: "row", alignItems: "flex-start", gap: 8 }}>
        <View style={{ width: 16, height: 16, flexShrink: 0 }}>
          <If
            when={props.job().done}
            else={
              <IconSpin
                name="loader"
                size={16}
                color={paletteToken("accent_fg")}
                spin={true}
                defer={false}
              />
            }
          >
            <Icon name="check" size={16} color={paletteToken("ok")} />
          </If>
        </View>
        <View style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", gap: 2 }}>
          <Text style={{ fontSize: 14, color: paletteToken("fg") }}>{props.job().title}</Text>
          <If when={props.job().detail !== undefined && props.job().detail !== ""}>
            <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>{props.job().detail}</Text>
          </If>
          <If when={!props.job().done && props.now() - props.job().startedAt >= SLOW_MS}>
            <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>{t("wait.takingLonger")}</Text>
          </If>
        </View>
        <View
          testID={`job-close-${props.job().id}`}
          accessibilityLabel={
            props.job().done || !props.job().cancellable ? t("common.close") : t("common.cancel")
          }
          onPress={() => closeJob(props.job().id)}
          style={{ width: 16, height: 16, cursor: "pointer", flexShrink: 0 }}
        >
          <Icon name="x" size={14} color={paletteToken("fg_muted")} />
        </View>
      </View>
      <If when={props.job().fraction !== undefined}>
        <View
          style={{
            height: 4,
            borderRadius: 2,
            backgroundColor: paletteToken("well"),
            overflow: "hidden",
          }}
        >
          <View
            style={{
              height: 4,
              width: `${Math.max(0, Math.min(100, Math.round((props.job().fraction ?? 0) * 100)))}%`,
              backgroundColor: paletteToken("accent"),
            }}
          />
        </View>
      </If>
    </View>
  );
}

function closeJob(id: string): void {
  const job = jobList().find((item) => item.id === id);
  if (job && !job.done && job.cancellable) job.onCancel?.();
  dropJob(id);
}
