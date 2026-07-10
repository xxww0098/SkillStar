import type { ComponentType } from "react";
import type { SubscriptionUsage } from "../../types";
import { CursorUsagePanel } from "../panels/CursorUsagePanel";
import { DeepSeekUsagePanel } from "../panels/DeepSeekUsagePanel";
import { GlmUsagePanel } from "../panels/GlmUsagePanel";
import { GrokUsagePanel } from "../panels/GrokUsagePanel";
import { DefaultUsageBody } from "./DefaultUsageBody";

export interface UsageBodyProps {
  usage: SubscriptionUsage;
  catalogId: string;
  brandColorHex: string;
  /** Required — never rely on implicit defaults (float window must pass compact). */
  density: "comfortable" | "compact";
  context?: {
    hasPlatformToken?: boolean;
  };
}

export type UsageBodyComponent = ComponentType<UsageBodyProps>;

export interface BodyRegistration {
  component: UsageBodyComponent;
  /** Body renders credits itself (suppress generic CreditsPanel). */
  ownsCredits: boolean;
  /** Body renders balance itself (suppress generic BalancePanel). */
  ownsBalance: boolean;
  /**
   * Whether Meta should suppress primary ResetCountdown:
   * - true: body always owns it
   * - false: Meta may show
   * - "infer": use windowRendersOwnReset on primary window
   */
  ownsPrimaryReset: boolean | "infer";
}

/** Specialized catalog bodies — owns* must be explicit (never `in REGISTRY`). */
export const USAGE_BODY_REGISTRY: Record<string, BodyRegistration> = {
  deepseek: {
    component: DeepSeekUsagePanel as UsageBodyComponent,
    ownsCredits: true,
    ownsBalance: true,
    ownsPrimaryReset: false,
  },
  glm: {
    component: GlmUsagePanel as UsageBodyComponent,
    ownsCredits: true,
    ownsBalance: false,
    ownsPrimaryReset: "infer",
  },
  xai: {
    component: GrokUsagePanel as UsageBodyComponent,
    ownsCredits: true,
    ownsBalance: false,
    ownsPrimaryReset: "infer",
  },
  cursor: {
    component: CursorUsagePanel as UsageBodyComponent,
    ownsCredits: true,
    ownsBalance: false,
    ownsPrimaryReset: "infer",
  },
};

export function resolveUsageBodyRegistration(catalogId: string): BodyRegistration {
  return (
    USAGE_BODY_REGISTRY[catalogId] ?? {
      component: DefaultUsageBody,
      ownsCredits: false,
      ownsBalance: false,
      ownsPrimaryReset: "infer",
    }
  );
}
