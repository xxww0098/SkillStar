import { Image, View, signal, type JSX, type Signal } from "nsh";
import { Settings } from "skillstar-gui/native";
import { paletteToken } from "../services/themeMode";
import { Icon } from "./icons";

// Shared GitHub repository owner avatar component used at the top-left of
// SkillCard, MarketCard, PublisherCard, and the publisher detail hero. Reads
// disk-cached owner images (~/.skillstar/cache/avatars/{owner}.png) or fetches
// via the configured HTTP client on miss, falling back cleanly to a `gitBranch`
// repository icon (or `hardDrive` for local skills).

const GITHUB_OWNER_RE = /^[A-Za-z0-9_][A-Za-z0-9_-]{0,38}$/;

const avatarSignals = new Map<string, Signal<string | null>>();
const inFlightOwners = new Set<string>();
const failedOwners = new Set<string>();

/** Trim whitespace / leading `@` and validate a GitHub owner/org segment. */
export function sanitizeGithubOwner(raw: string | null | undefined): string | null {
  if (!raw) return null;
  const clean = raw.trim().replace(/^@+/, "");
  return GITHUB_OWNER_RE.test(clean) ? clean : null;
}

function ownerFromGitUrl(gitUrl: string | null | undefined): string | null {
  if (!gitUrl) return null;
  const marker = "github.com";
  const idx = gitUrl.toLowerCase().indexOf(marker);
  if (idx === -1) return null;
  const after = gitUrl.slice(idx + marker.length).replace(/^[/:]+/, "");
  const owner = after.split("/")[0]?.replace(/\.git$/i, "");
  return sanitizeGithubOwner(owner);
}

/** Resolve the GitHub owner handle for a skill's top-left repository icon. */
export function githubOwnerForSkill(skill: {
  skillType?: string;
  source?: string | null;
  author?: string | null;
  gitUrl?: string | null;
}): string | null {
  if (skill.skillType === "local") return null;

  const source = skill.source?.trim();
  if (source && source !== "local" && source !== "remote" && source.includes("/")) {
    const owner = sanitizeGithubOwner(source.split("/")[0]);
    if (owner !== null) return owner;
  }

  const fromUrl = ownerFromGitUrl(skill.gitUrl);
  if (fromUrl !== null) return fromUrl;

  return sanitizeGithubOwner(skill.author);
}

/** Reactive per-owner avatar URI backed by native disk cache + HTTP fetch.
 * Read only from JSX expressions or effects so nsh tracks updates. */
export function resolveRepoAvatarUri(owner: string | null | undefined): string | null {
  const clean = sanitizeGithubOwner(owner);
  if (clean === null) return null;

  let sig = avatarSignals.get(clean);
  if (!sig) {
    sig = signal<string | null>(Settings.peekGithubAvatarDataUri(clean) ?? null);
    avatarSignals.set(clean, sig);
  }
  const current = sig();
  if (current === null && !inFlightOwners.has(clean) && !failedOwners.has(clean)) {
    inFlightOwners.add(clean);
    const target = sig;
    void Settings.loadGithubAvatarDataUri(clean)
      .then((loaded) => {
        if (loaded !== null) {
          target.set(loaded);
        } else {
          failedOwners.add(clean);
        }
      })
      .catch(() => {
        failedOwners.add(clean);
      })
      .finally(() => {
        inFlightOwners.delete(clean);
      });
  }
  return current;
}

/** Rounded repository owner avatar with `gitBranch` / `hardDrive` fallback. */
export function RepoAvatar(props: {
  owner: string | null;
  local?: boolean;
  selected?: boolean;
  size?: number;
  radius?: number;
  testID?: string;
}): JSX.Element {
  return (
    <View
      testID={props.testID}
      style={{
        width: props.size ?? 32,
        height: props.size ?? 32,
        borderRadius: props.radius ?? 9,
        overflow: "hidden",
        flexShrink: 0,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        borderWidth: 1,
        borderColor: props.selected
          ? paletteToken("accent")
          : props.local
            ? paletteToken("ok_border")
            : paletteToken("border"),
        backgroundColor: props.local ? paletteToken("ok_bg") : paletteToken("well"),
      }}
    >
      <If
        when={Boolean(props.local)}
        else={
          <If
            when={resolveRepoAvatarUri(props.owner) !== null}
            else={
              <Icon
                name="gitBranch"
                size={Math.round((props.size ?? 32) * 0.48)}
                color={paletteToken("fg_muted")}
              />
            }
          >
            <Image
              src={resolveRepoAvatarUri(props.owner) ?? ""}
              fadeDuration={0}
              style={{
                width: "100%",
                height: "100%",
                objectFit: "cover",
              }}
            />
          </If>
        }
      >
        <Icon
          name="hardDrive"
          size={Math.round((props.size ?? 32) * 0.48)}
          color={paletteToken("ok")}
        />
      </If>
    </View>
  );
}
