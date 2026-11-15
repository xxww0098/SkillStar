import { Text, View, type JSX } from "nsh";
import { isLight, paletteToken } from "../services/themeMode";
import { Icon, type IconName } from "./icons";

// Tone callouts — chrome::info_alert & co parity (chrome/mod.rs:59-122).
// Light mode pins each tone to its opaque palette surface (the 4%-alpha wash
// the kit derives would float over page content); dark mode keeps the subtle
// tone-tinted wash. Shared by the notice band and in-page error callouts.

export type AlertTone = "info" | "success" | "warning" | "error";

interface ToneSurface {
  icon: IconName;
  ink: string;
  bg: string;
  border: string;
}

function toneSurface(tone: AlertTone): ToneSurface {
  if (isLight()) {
    // Light mode: opaque palette pairs (theme.rs contrast tests guard them).
    switch (tone) {
      case "info":
        return {
          icon: "info",
          ink: paletteToken("info"),
          bg: paletteToken("info_bg"),
          border: paletteToken("info_border"),
        };
      case "success":
        return {
          icon: "checkCircle",
          ink: paletteToken("ok"),
          bg: paletteToken("ok_bg"),
          border: paletteToken("ok_border"),
        };
      case "warning":
        return {
          icon: "alertTriangle",
          ink: paletteToken("warn"),
          bg: paletteToken("warn_bg"),
          border: paletteToken("warn_border"),
        };
      case "error":
        return {
          icon: "xCircle",
          ink: paletteToken("danger_fg"),
          bg: paletteToken("danger_bg"),
          border: paletteToken("danger_border"),
        };
    }
  }
  // Dark mode: the tone's soft wash over dark panels.
  switch (tone) {
    case "info":
      return {
        icon: "info",
        ink: paletteToken("info"),
        bg: "rgba(88,166,255,0.08)",
        border: "rgba(88,166,255,0.30)",
      };
    case "success":
      return {
        icon: "checkCircle",
        ink: paletteToken("ok"),
        bg: "rgba(45,212,191,0.08)",
        border: "rgba(45,212,191,0.30)",
      };
    case "warning":
      return {
        icon: "alertTriangle",
        ink: paletteToken("warn"),
        bg: "rgba(251,191,36,0.08)",
        border: "rgba(251,191,36,0.30)",
      };
    case "error":
      return {
        icon: "xCircle",
        ink: paletteToken("danger_fg"),
        bg: "rgba(248,81,73,0.08)",
        border: "rgba(248,81,73,0.30)",
      };
  }
}

export function Alert(props: {
  tone: AlertTone;
  message: string;
  /** Copy ceiling for band rows (NOTICE_MAX_W = 560). */
  maxWidth?: number;
  onClose?: () => void;
}): JSX.Element {
  const surface = toneSurface(props.tone);
  return (
    <View
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        gap: 8,
        paddingHorizontal: 12,
        paddingVertical: 8,
        borderRadius: 8,
        borderWidth: 1,
        backgroundColor: surface.bg,
        borderColor: surface.border,
        maxWidth: props.maxWidth,
      }}
    >
      <Icon name={surface.icon} size={14} color={surface.ink} />
      <Text style={{ fontSize: 12, lineHeight: 18, color: surface.ink }}>{props.message}</Text>
      {props.onClose ? (
        <View
          style={{ display: "flex", padding: 2, cursor: "pointer" }}
          hoverStyle={{ opacity: 0.7 }}
          onPress={props.onClose}
        >
          <Icon name="x" size={12} color={surface.ink} />
        </View>
      ) : null}
    </View>
  );
}

// In-page callout shortcuts (the pages' error banners use these shapes).
export function infoAlert(message: string): JSX.Element {
  return <Alert tone="info" message={message} />;
}
export function successAlert(message: string): JSX.Element {
  return <Alert tone="success" message={message} />;
}
export function warningAlert(message: string): JSX.Element {
  return <Alert tone="warning" message={message} />;
}
export function errorAlert(message: string): JSX.Element {
  return <Alert tone="error" message={message} />;
}
