import { Image, List, Text, View, computed, signal, type JSX } from "nsh";
import { Switch } from "nsh/base";
import { cardTrackWidth, gridColumnsForPane, packCardRows } from "../../chrome/cardShell";
import { bindDelayedFlag } from "../../chrome/delayedFlag";
import { Pulse } from "../../chrome/feedback";
import { Icon } from "../../chrome/icons";
import { settleTransition } from "../../chrome/motion";
import { resolveRepoAvatarUri, sanitizeGithubOwner } from "../../chrome/repoAvatar";
import { BarSegmentSlot, BarSegmented } from "../../chrome/toolbar";
import { Tip } from "../../chrome/tooltip";
import { CARD_GAP, CARD_ROW_W, PAGE_PAD } from "../../layout";
import { paletteToken } from "../../services/themeMode";
import { ms } from "./mcpStrings";
import type { McpState, StoreFilter } from "./state";
import type { StoreItem } from "./types";

const STORE_CARD_H = 152;
const STORE_FOOTER_H = 38;

function githubOwnerFromStoreItem(item: StoreItem | undefined): string | null {
  if (!item) return null;
  const icon = item.icon.trim();
  const ghIconMatch = /^https?:\/\/github\.com\/([^/?#]+)\.png/i.exec(icon);
  if (ghIconMatch) {
    const owner = sanitizeGithubOwner(ghIconMatch[1]);
    if (owner) return owner;
  }
  const home = item.homepage.trim();
  const marker = "github.com/";
  const idx = home.toLowerCase().indexOf(marker);
  if (idx !== -1) {
    const seg = home.slice(idx + marker.length).split("/")[0];
    const owner = sanitizeGithubOwner(seg);
    if (owner) return owner;
  }
  return null;
}

function storeAvatarSrc(item: StoreItem | undefined): string | null {
  if (!item) return null;
  const owner = githubOwnerFromStoreItem(item);
  if (owner) {
    const cached = resolveRepoAvatarUri(owner);
    if (cached) return cached;
  }
  const rawIcon = item.icon.trim();
  if (rawIcon.length > 0 && !/^https?:\/\/github\.com\/[^/?#]+\.png/i.test(rawIcon)) {
    return rawIcon;
  }
  return null;
}

export function McpStoreView(props: { state: McpState }): JSX.Element {
  const paneWidth = signal(0);
  const columns = computed(() =>
    gridColumnsForPane(paneWidth(), false, PAGE_PAD, CARD_ROW_W),
  );
  const trackW = () => cardTrackWidth(paneWidth(), false, PAGE_PAD, CARD_ROW_W);
  const packedRows = computed(() =>
    packCardRows(props.state.filteredStoreItems(), columns()),
  );
  const skeletonGate = bindDelayedFlag(
    () => props.state.storeLoading() && props.state.storeItems().length === 0,
  );
  const showSkeleton = () =>
    skeletonGate() && props.state.storeLoading() && props.state.storeItems().length === 0;
  const showEmpty = () =>
    !props.state.storeLoading() && props.state.filteredStoreItems().length === 0;

  return (
    <View
      style={{ display: "flex", flexDirection: "column", flex: 1, minHeight: 0, width: "100%" }}
      onLayout={(event) => {
        const w = event.bounds.size.width;
        if (w > 0 && w !== paneWidth()) paneWidth.set(w);
      }}
    >
      <StoreFilterBar state={props.state} />

      <View style={{ flex: 1, minHeight: 0, width: "100%" }}>
        <If when={showSkeleton()}>
          <StoreSkeletonGrid cardWidth={trackW()} />
        </If>

        <If when={showEmpty()}>
          <StoreEmptyState />
        </If>

        <If when={!showSkeleton() && props.state.filteredStoreItems().length > 0}>
          <List
            each={packedRows()}
            keyExtractor={(row) => row.map((item) => item.id).join("\u001e")}
            estimatedItemSize={STORE_CARD_H + CARD_GAP}
            scrollEventThrottle={32}
            style={{ width: "100%", height: "100%" }}
            contentStyle={{
              paddingTop: 4,
              paddingLeft: PAGE_PAD,
              paddingRight: PAGE_PAD,
              paddingBottom: 20,
            }}
          >
            {(row) => (
              <View
                style={{
                  display: "flex",
                  flexDirection: "row",
                  alignItems: "stretch",
                  gap: CARD_GAP,
                  paddingBottom: CARD_GAP,
                  width: "100%",
                }}
              >
                <Each of={row()} key="index">
                  {(item: () => StoreItem) => (
                    <StoreItemCard state={props.state} item={item} cardWidth={trackW()} />
                  )}
                </Each>
              </View>
            )}
          </List>
        </If>
      </View>
    </View>
  );
}

function StoreFilterBar(props: { state: McpState }): JSX.Element {
  const errHovered = signal(false);
  const filters: { key: StoreFilter; label: () => string }[] = [
    { key: "all", label: () => ms().storeFilterAll },
    { key: "featured", label: () => ms().storeFilterFeatured },
    { key: "stdio", label: () => ms().storeFilterStdio },
    { key: "remote", label: () => ms().storeFilterRemote },
  ];

  return (
    <View
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        justifyContent: "space-between",
        gap: 12,
        height: 40,
        paddingHorizontal: PAGE_PAD,
        flexShrink: 0,
      }}
    >
      <BarSegmented
        testID="mcp-store-filters"
        slots={filters.map((f) => (
          <BarSegmentSlot
            testID={`mcp-store-filter-${f.key}`}
            tone="filter"
            label={f.label()}
            selected={props.state.storeFilter() === f.key}
            onPress={() => props.state.storeFilter.set(f.key)}
          />
        ))}
      />

      <View
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          gap: 10,
          flexShrink: 1,
          minWidth: 0,
        }}
      >
        <If when={props.state.storeOnlineError() !== null}>
          <View
            tooltip={
              errHovered() && props.state.storeOnlineError()
                ? <Tip text={props.state.storeOnlineError() ?? ""} />
                : undefined
            }
            onMouseEnter={() => errHovered.set(true)}
            onMouseLeave={() => errHovered.set(false)}
            style={{
              display: "flex",
              flexDirection: "row",
              alignItems: "center",
              gap: 5,
              maxWidth: 260,
              flexShrink: 1,
              minWidth: 0,
              paddingHorizontal: 8,
              height: 26,
              borderRadius: 8,
              borderWidth: 1,
              borderColor: paletteToken("warn_border"),
              backgroundColor: paletteToken("warn_bg"),
            }}
          >
            <Icon name="alertTriangle" size={12} color={paletteToken("warn")} />
            <Text
              style={{
                fontSize: 11,
                color: paletteToken("warn"),
                numberOfLines: 1,
                textOverflow: "ellipsis",
                flexShrink: 1,
                minWidth: 0,
              }}
            >
              {props.state.storeOnlineError() ?? ""}
            </Text>
          </View>
        </If>

        <View
          style={{
            display: "flex",
            flexDirection: "row",
            alignItems: "center",
            gap: 8,
            flexShrink: 0,
          }}
        >
          <Text style={{ fontSize: 12, fontWeight: "500", color: paletteToken("fg_muted") }}>
            {ms().storeOnlineLabel}
          </Text>
          <Switch
            testID="mcp-store-online-toggle"
            checked={props.state.storeOnline()}
            accessibilityLabel={ms().storeOnlineToggle}
            trackStyle={{
              width: 28,
              height: 16,
              borderRadius: 8,
              borderWidth: 1,
              backgroundColor: props.state.storeOnline()
                ? paletteToken("ok")
                : paletteToken("input"),
              borderColor: props.state.storeOnline()
                ? paletteToken("ok_border")
                : paletteToken("edge"),
            }}
            thumbStyle={{
              width: 12,
              height: 12,
              borderRadius: 6,
              backgroundColor: paletteToken("on_accent"),
              transform: props.state.storeOnline() ? "translateX(12px)" : "translateX(0px)",
              ...settleTransition(),
            }}
            onChange={(e: { checked: boolean }) => {
              props.state.storeOnline.set(e.checked);
              void props.state.searchStore(props.state.storeQuery(), e.checked);
            }}
          />
        </View>
      </View>
    </View>
  );
}

function StoreSkeletonGrid(props: { cardWidth: number }): JSX.Element {
  return (
    <View
      testID="mcp-store-skeletons"
      accessibilityState={{ busy: true }}
      style={{
        display: "flex",
        flexDirection: "row",
        flexWrap: "wrap",
        gap: CARD_GAP,
        justifyContent: "flex-start",
        alignItems: "flex-start",
        paddingTop: 4,
        paddingBottom: 20,
        paddingHorizontal: PAGE_PAD,
      }}
    >
      {[0, 1, 2, 3, 4, 5].map((index) => (
        <View
          style={{
            width: props.cardWidth,
            height: STORE_CARD_H,
            flexShrink: 0,
            borderRadius: 16,
            borderWidth: 1,
            borderColor: paletteToken("border"),
            backgroundColor: paletteToken("card"),
            overflow: "hidden",
            display: "flex",
            flexDirection: "column",
            justifyContent: "space-between",
          }}
        >
          <View style={{ display: "flex", flexDirection: "column", gap: 10, padding: 14 }}>
            <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 10 }}>
              <Pulse
                id={`mcp-store-skel-${index}-avatar`}
                style={{ width: 32, height: 32, borderRadius: 9, flexShrink: 0 }}
              />
              <View style={{ display: "flex", flexDirection: "column", flex: 1, gap: 5 }}>
                <Pulse
                  id={`mcp-store-skel-${index}-title`}
                  style={{ width: "58%", height: 13, borderRadius: 5 }}
                />
                <Pulse
                  id={`mcp-store-skel-${index}-pub`}
                  secondary
                  style={{ width: "36%", height: 10, borderRadius: 4 }}
                />
              </View>
            </View>
            <View style={{ display: "flex", flexDirection: "column", gap: 5 }}>
              <Pulse
                id={`mcp-store-skel-${index}-d1`}
                secondary
                style={{ width: "92%", height: 11, borderRadius: 4 }}
              />
              <Pulse
                id={`mcp-store-skel-${index}-d2`}
                secondary
                style={{ width: "68%", height: 11, borderRadius: 4 }}
              />
            </View>
          </View>
          <View
            style={{
              height: STORE_FOOTER_H,
              paddingHorizontal: 14,
              borderTopWidth: 1,
              borderTopColor: paletteToken("border_soft"),
              backgroundColor: paletteToken("well"),
              display: "flex",
              flexDirection: "row",
              alignItems: "center",
              justifyContent: "space-between",
            }}
          >
            <Pulse
              id={`mcp-store-skel-${index}-meta`}
              secondary
              style={{ width: 88, height: 12, borderRadius: 4 }}
            />
            <Pulse
              id={`mcp-store-skel-${index}-cta`}
              style={{ width: 64, height: 24, borderRadius: 7 }}
            />
          </View>
        </View>
      ))}
    </View>
  );
}

function StoreEmptyState(): JSX.Element {
  return (
    <View
      style={{
        display: "flex",
        flex: 1,
        width: "100%",
        flexDirection: "column",
        alignItems: "center",
        justifyContent: "center",
        gap: 10,
        paddingVertical: 40,
        paddingHorizontal: PAGE_PAD,
      }}
    >
      <View
        style={{
          width: 52,
          height: 52,
          borderRadius: 999,
          backgroundColor: paletteToken("well"),
          borderWidth: 1,
          borderColor: paletteToken("border"),
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
        }}
      >
        <Icon name="store" size={20} color={paletteToken("fg_muted")} />
      </View>
      <Text style={{ fontSize: 13, color: paletteToken("fg_muted"), textAlign: "center" }}>
        {ms().storeEmpty}
      </Text>
    </View>
  );
}

function StoreItemAvatar(props: {
  src: () => string | null;
  transport: () => StoreItem["transport"];
}): JSX.Element {
  return (
    <View
      style={{
        width: 32,
        height: 32,
        borderRadius: 9,
        overflow: "hidden",
        flexShrink: 0,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        borderWidth: 1,
        borderColor: paletteToken("border"),
        backgroundColor: paletteToken("well"),
      }}
    >
      <If
        when={props.src() !== null}
        else={
          <Icon
            name={props.transport() === "stdio" ? "terminal" : "globe"}
            size={15}
            color={paletteToken("fg_muted")}
          />
        }
      >
        <Image
          src={props.src() ?? ""}
          fadeDuration={0}
          style={{
            width: "100%",
            height: "100%",
            objectFit: "cover",
          }}
        />
      </If>
    </View>
  );
}

function StoreItemCard(props: {
  state: McpState;
  item: () => StoreItem;
  cardWidth: number;
}): JSX.Element {
  const avatarSrc = computed(() => storeAvatarSrc(props.item()));
  const have = () => props.item().have;
  const id = () => props.item().id;
  const runnerMeta = () => {
    const cur = props.item();
    const tr = cur.transport.toUpperCase();
    return cur.runs ? `${tr} · ${cur.runs}` : tr;
  };

  return (
    <View
      testID={`mcp-store-card-${id()}`}
      style={{
        width: props.cardWidth,
        flexShrink: 0,
        minHeight: STORE_CARD_H,
        display: "flex",
        flexDirection: "column",
        justifyContent: "space-between",
        borderRadius: 16,
        borderWidth: 1,
        borderColor: paletteToken("border"),
        backgroundColor: paletteToken("card"),
        overflow: "hidden",
      }}
      hoverStyle={{
        backgroundColor: paletteToken("card_hover"),
        borderColor: paletteToken("edge"),
      }}
    >
      <View
        style={{
          display: "flex",
          flexDirection: "column",
          gap: 8,
          padding: 14,
          paddingBottom: 10,
          flex: 1,
          minWidth: 0,
        }}
      >
        <View
          style={{
            display: "flex",
            flexDirection: "row",
            alignItems: "center",
            gap: 10,
          }}
        >
          <StoreItemAvatar src={avatarSrc} transport={() => props.item().transport} />
          <View style={{ flex: 1, minWidth: 0, gap: 2 }}>
            <View
              style={{
                display: "flex",
                flexDirection: "row",
                alignItems: "center",
                justifyContent: "space-between",
                gap: 6,
              }}
            >
              <Text
                style={{
                  flex: 1,
                  minWidth: 0,
                  fontSize: 13,
                  fontWeight: "600",
                  color: paletteToken("fg"),
                  numberOfLines: 1,
                  textOverflow: "ellipsis",
                }}
              >
                {props.item().title || id()}
              </Text>
              <If when={props.item().featured === true}>
                <View
                  style={{
                    paddingHorizontal: 7,
                    paddingVertical: 2,
                    borderRadius: 6,
                    backgroundColor: paletteToken("accent_soft"),
                    flexShrink: 0,
                  }}
                >
                  <Text style={{ fontSize: 10, fontWeight: "600", color: paletteToken("accent_fg") }}>
                    {ms().storeFeatured}
                  </Text>
                </View>
              </If>
            </View>
            <Text
              style={{
                fontSize: 11,
                color: paletteToken("fg_muted"),
                numberOfLines: 1,
                textOverflow: "ellipsis",
              }}
            >
              {props.item().publisher || id()}
            </Text>
          </View>
        </View>
        <Text
          style={{
            fontSize: 12,
            lineHeight: 17,
            color: paletteToken("fg_muted"),
            numberOfLines: 2,
            textOverflow: "ellipsis",
          }}
        >
          {props.item().description}
        </Text>
      </View>

      <View
        style={{
          height: STORE_FOOTER_H,
          flexShrink: 0,
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          justifyContent: "space-between",
          gap: 8,
          paddingHorizontal: 14,
          borderTopWidth: 1,
          borderTopColor: paletteToken("border_soft"),
          backgroundColor: paletteToken("well"),
        }}
      >
        <View
          style={{
            display: "flex",
            flexDirection: "row",
            alignItems: "center",
            gap: 6,
            flex: 1,
            minWidth: 0,
          }}
        >
          <Text
            style={{
              fontSize: 11,
              fontFamily: "monospace",
              color: paletteToken("fg_muted"),
              numberOfLines: 1,
              textOverflow: "ellipsis",
              flexShrink: 1,
              minWidth: 0,
            }}
          >
            {runnerMeta()}
          </Text>
          <If when={props.item().signIn === true}>
            <View
              style={{
                paddingHorizontal: 6,
                paddingVertical: 1,
                borderRadius: 5,
                borderWidth: 1,
                borderColor: paletteToken("warn_border"),
                backgroundColor: paletteToken("warn_bg"),
                flexShrink: 0,
              }}
            >
              <Text style={{ fontSize: 10, fontWeight: "500", color: paletteToken("warn") }}>
                {ms().storeSignInBadge}
              </Text>
            </View>
          </If>
        </View>

        <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 6, flexShrink: 0 }}>
          <If when={Boolean(have())}>
            <View
              testID={`mcp-store-clone-${id()}`}
              role="button"
              accessibilityLabel={ms().storeCloneBtn}
              tooltip={<Tip text={ms().storeCloneBtn} />}
              style={{
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                width: 24,
                height: 24,
                borderRadius: 7,
                borderWidth: 1,
                borderColor: paletteToken("border"),
                backgroundColor: paletteToken("card"),
                cursor: "pointer",
                flexShrink: 0,
              }}
              hoverStyle={{
                backgroundColor: paletteToken("panel_hover"),
                borderColor: paletteToken("edge"),
              }}
              onPress={() => props.state.triggerInstallStoreItem(props.item(), true)}
            >
              <Icon name="copy" size={11} color={paletteToken("fg_muted")} />
            </View>
          </If>
          <View
            testID={`mcp-store-install-${id()}`}
            role="button"
            style={{
              display: "flex",
              flexDirection: "row",
              alignItems: "center",
              gap: 4,
              height: 24,
              maxWidth: 154,
              paddingHorizontal: 10,
              borderRadius: 7,
              borderWidth: have() ? 1 : 0,
              borderColor: have() ? paletteToken("border") : "transparent",
              backgroundColor: have() ? paletteToken("card") : paletteToken("accent"),
              cursor: "pointer",
              flexShrink: 0,
            }}
            hoverStyle={{
              backgroundColor: have() ? paletteToken("panel_hover") : paletteToken("accent_hover"),
              borderColor: have() ? paletteToken("edge") : "transparent",
            }}
            onPress={() => props.state.triggerInstallStoreItem(props.item())}
          >
            <If when={Boolean(have())}>
              <Icon name="check" size={11} color={paletteToken("ok")} />
            </If>
            <Text
              style={{
                fontSize: 11,
                fontWeight: "600",
                color: have() ? paletteToken("fg") : paletteToken("on_accent"),
                numberOfLines: 1,
                textOverflow: "ellipsis",
                flexShrink: 1,
                minWidth: 0,
              }}
            >
              {have() ? `${ms().storeInstalled} (${have()})` : ms().storeInstall}
            </Text>
          </View>
        </View>
      </View>
    </View>
  );
}
