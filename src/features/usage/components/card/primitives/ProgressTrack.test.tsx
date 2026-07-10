import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { brandUrgencyFillClass, ProgressTrack } from "./ProgressTrack";

describe("brandUrgencyFillClass", () => {
  it("uses rose pulse at >= 90", () => {
    const c = brandUrgencyFillClass(90);
    expect(c).toContain("from-rose-500");
    expect(c).toContain("animate-pulse");
  });

  it("uses amber at >= 75 and < 90", () => {
    const c = brandUrgencyFillClass(75);
    expect(c).toContain("from-amber-500");
    expect(c).not.toContain("animate-pulse");
  });

  it("uses brand CSS vars below 75", () => {
    const c = brandUrgencyFillClass(40);
    expect(c).toContain("from-[var(--brand-color)]");
    expect(c).toContain("to-[var(--brand-color-2)]");
  });
});

describe("ProgressTrack", () => {
  it("renders brand-urgency track with remaining-oriented width", () => {
    const { getByTestId } = render(
      <ProgressTrack usedPercent={25} size="compact" tone="brand-urgency" data-testid="track" />,
    );
    const track = getByTestId("track");
    expect(track).toHaveAttribute("data-tone", "brand-urgency");
    expect(track).toHaveAttribute("data-size", "compact");
    expect(track.className).toContain("h-1.5");
    const fill = getByTestId("track-fill");
    // 25% used → 75% remaining width
    expect(fill).toHaveStyle({ width: "75%" });
  });

  it("comfortable size uses h-2", () => {
    const { getByTestId } = render(
      <ProgressTrack usedPercent={0} size="comfortable" tone="brand-urgency" data-testid="track" />,
    );
    expect(getByTestId("track").className).toContain("h-2");
  });

  it("billing-used tone sets data attribute", () => {
    const { getByTestId } = render(
      <ProgressTrack usedPercent={10} size="comfortable" tone="billing-used" resetAt={null} data-testid="track" />,
    );
    expect(getByTestId("track")).toHaveAttribute("data-tone", "billing-used");
  });

  it("accent-static applies gradient via style", () => {
    const { getByTestId } = render(
      <ProgressTrack usedPercent={50} size="compact" tone="accent-static" accent="#00E5BC" data-testid="track" />,
    );
    const fill = getByTestId("track-fill");
    // jsdom may normalize hex to rgb() in style.background
    expect(fill.style.background).toMatch(/linear-gradient\(90deg/);
    expect(fill.style.background).toMatch(/#00E5BC|rgb\(0,\s*229,\s*188\)/);
    expect(fill).toHaveStyle({ width: "50%" });
  });
});
